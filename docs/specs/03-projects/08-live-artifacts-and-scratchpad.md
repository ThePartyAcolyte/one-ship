# Live Artifacts, Scratchpad & Documentation Workspace

## Purpose

Define ONE-SHIP's file-backed document lifecycle across `docs/` and `scratchpad/`, filesystem watch synchronization, discrete topical conversation persistence, inline document review commenting with voice dictation, sandboxed Explorer file operations, and the integrated CodeMirror 6 reader/editor workspace.

## Scope

In scope:
- `docs/` vs `scratchpad/` lifecycle, `.gitignore` / Git safety snapshot interaction, and `vcs.scratchpad_retention_days`.
- Agent `post_agent_response` `file_path` binding and `project-files-changed` filesystem watch notifications.
- Unified Reader, Terminal Log, and Three-Tier Navigator (`scratchpad`, `docs/`, ephemeral artifacts).
- Topical conversations (`agent_conversations`, `agent_chat_turns`, `switch_conversation_topic`, 60-line terminal snapshot audit).
- Inline line-range review comments (`document_comments`), voice mic capture, structured `[REVIEW CONTEXT]` prompt packaging, and MCP resolution (`list_document_comments`, `resolve_document_comment`).
- Sandboxed Explorer file/directory operations (`Docs` vs `Codebase` scopes, multi-select clipboard, modals).
- In-place CodeMirror 6 viewer/editor (`AppCodeEditor.vue`), real-time syntax diagnostics, Tree-sitter/Markdown outline, and indexed symbol definition navigation.

Out of scope:
- Project initialization docstyle selection workflows ([Greenfield Project Creation](10-greenfield-project-creation.md)).
- Knowledge graph indexing internals and ONNX chunking (covered in the knowledge platform specs).

## Model / Design

### 1. File-Backed Document Architecture (`docs/` & `scratchpad/`)

Rather than embedding large plans and specifications inline inside single-turn chat payloads, ONE-SHIP treats artifacts as first-class files inside the project directory:

```text
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
│   │            ONE-SHIP Daemon & Watcher           │                   │
│   │  - Binds live artifact to project relative path│                   │
│   │  - Emits file modification refresh events      │                   │
│   └──────────────────────┬─────────────────────────┘                   │
│                          │ SSE Event (`project-files-changed`)         │
│                          ▼                                             │
│   ┌────────────────────────────────────────────────┐                   │
│   │           Workspace Reader / Editor            │                   │
│   │  - Mode 1: [Live Artifact] (auto-refreshing)   │                   │
│   │  - Mode 2: [Browse Docs] (docs/ & scratchpad/) │                   │
│   │  - Rich Markdown / CodeMirror 6 / Multi-format │                   │
│   └────────────────────────────────────────────────┘                   │
└────────────────────────────────────────────────────────────────────────┘
```

#### `docs/` (Stable Knowledge Base)

- **Purpose:** Permanent project specifications, architecture documents, runbooks, schemas, and developer manuals.
- **Git strategy:** Fully versioned in the project repository.
- **Scaffolding lifecycle:** During brownfield Phase 1 initialization, `docs/` is **not** eagerly or automatically created. When documentation is absent or incomplete, the initializer raises `flag_type = 'docs'`, and `docs/` is scaffolded in Phase 2 after the developer selects a Docstyle.
- **Access & UI:** Read/write for humans and requested agents; browsable as an infinite recursive tree inside the workspace Explorer (`Docs` scope).

#### `scratchpad/` (Ephemeral Agent Workspace)

- **Purpose:** Iterative plans, draft breakdowns, debug notes, profiler exports, and intermediate research summaries.
- **Lifecycle & retention:**
  - Auto-created at project root on first agent execution (`ensure_project_scratchpad_dir`).
  - Recommended for inclusion in `.gitignore` so transient drafts do not pollute commit history.
  - Covered by ONE-SHIP's hidden Git safety snapshots for rollback and auditability without dirtying `git status`.
  - Managed via standard Explorer file actions (select, rename, delete) without a special-purpose purge button; optional `vcs.scratchpad_retention_days` (default `0` = disabled) performs age-based pruning when positive.

### 2. Agent Operational Protocol & Live Watch

1. Agents write substantial plans, specs, or working notes to `scratchpad/<name>.md` or `docs/<name>.md`.
2. In `post_agent_response`, the agent passes the project-relative path in `file_path` (e.g. `"scratchpad/migration_plan.md"`) or inline companion content in `artifact`.
3. Subsequent updates use incremental file editing tools rather than re-emitting entire documents in dialogue.
4. `post_agent_response` resolves the canonical path inside the project root, and the per-project `notify` filesystem watcher (`ensure_project_files_watcher` in `kopeng/documents.rs`) emits `project-files-changed` over SSE so open documents refresh in place and disk vs. unsaved-draft conflicts are detected.

### 3. Reader, Terminal Log & Three-Tier Navigator

- **Header modes:** Switches between the full-width `[Reader]` canvas and `[Terminal Log]` (live rolling output of the active agent's supervised process with `All` vs `Errors Only` filtering and copy action).
- **Three-Tier Navigator:**
  1. **Active Scratchpads:** `scratchpad/*.md` files generated or touched in the active conversation topic.
  2. **Project Docs (`docs/`):** Recursive directory tree (`docs/...`) with expand/collapse states and `[● MODIFIED]` indicators.
  3. **Ephemeral Artifacts:** In-memory tables, plans, diffs, or code snippets from conversation turns, tagged `[EPHEMERAL]`.
- **Single-canvas reader:** Full-width Markdown renderer (`.md`) and monospaced code/text/diff viewer with line numbers, copy-to-clipboard, and auto-refresh on disk updates.

### 4. Discrete Topical Conversations & Turn Audit Trail

Conversations are partitioned into discrete topical sessions per agent in SQLite (`agent_conversations` and `agent_chat_turns`) to prevent token bloat and cross-task context contamination:

```text
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

- **`switch_conversation_topic` (MCP):** Transitions or resumes topical sessions cleanly, optionally storing a transition `summary` as a system turn in the target topic and emitting `conversation-topic-switched`.
- **Turn terminal audit:** Each agent turn samples the latest 60 lines of the agent's supervisor output buffer (`get_terminal_output`) into `agent_chat_turns.terminal_snapshot`.
- **History paging:** Chat overlays load older history incrementally via `loadOlderMessages`.

### 5. Inline Document Commenting & Agent Feedback Loop

#### Database Schema (`document_comments` in `oneship.db`)

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
CREATE INDEX IF NOT EXISTS idx_doc_comments_proj_file ON document_comments(project_id, file_path, status);
CREATE INDEX IF NOT EXISTS idx_doc_comments_status ON document_comments(status);
```

#### Selection, Voice Dictation & Review Packaging

1. `useDocumentComments` intercepts text selection in the document reader/editor and computes exact line ranges `L<start>-L<end>` from character offsets and newline counts.
2. A floating popover displays the line-range badge, quoted snippet, `AppTextarea`, native dictation microphone (`dictationStore` + `dictation-finished`), `[Save]`, and `[Cancel]`.
3. Saving calls `create_document_comment` and opens the Comments Drawer.
4. Developers attach open reviewed files via `[+ Attach to Chat]` into the Attached Review Files Tray. On prompt dispatch, `dialogueFormat.ts` formats an atomic review context block and detaches the files automatically:

   ```markdown
   [REVIEW CONTEXT: N FILE(S) ATTACHED]

   ### File: <rel_path>
   - [Comment #<id>] Lines L<start>-L<end>: "<selected_snippet>"
     Feedback: "<comment_text>"

   ---
   User Note: <user_input_text>
   ```

5. Agents inspect feedback with `list_document_comments` and mark items resolved via `resolve_document_comment`, emitting `DocumentCommentResolved` over SSE to update the Comments Drawer in real time.

### 6. Sandboxed Explorer File & Directory Operations

- **Scope selector (`Docs` vs `Codebase`):**
  - `Docs`: scans `docs/` and `scratchpad/` only.
  - `Codebase`: recursively scans the repository while ignoring build/VCS artifacts (`.git`, `target`, `node_modules`, `dist`, `.gemini`, `brain`, `package-lock.json`, `Cargo.lock`, etc.).
- **Path sandboxing:** Every operation resolves and canonicalizes paths via `sanitize_project_rel_path`, rejecting `..` traversal and guaranteeing operations remain strictly inside the project root.
- **Multi-select & clipboard (`projectExplorerStore.ts`):**
  - `[Select]` mode displays checkboxes and a docked bar (`[Select All]`, `[Clear]`, `[Copy]`, `[Cut]`, `[Delete]`).
  - `explorerClipboard` tracks selected paths (`copy` vs `cut`); `[Paste (N)]` invokes `copy_project_items` or `move_project_items` into the active directory.
  - Modals handle folder creation (`create_project_directory`) and batch deletion confirmation (`delete_project_items`).

### 7. In-Place Document & Code Editor (`AppCodeEditor.vue`)

- **Dual-mode workflow:**
  - **View / Review mode (default):** Formatted GFM HTML with highlighted code blocks and selection commenting for `.md`; readonly CodeMirror 6 with line numbers, syntax diagnostics, symbol navigation, and right-click context menu for code/config files.
  - **Edit mode (`[Edit]` / `Ctrl+E`):** Interactive full-height CodeMirror 6 surface (`AppCodeEditor.vue`) styled with semantic design tokens (`--ui-bg-*`, `--accent-color`, `--border-color`, `--status-*`).
- **Multi-tab & controls:** Open tabs (`openDocuments`) with horizontal wheel scroll, relative paths (`.` symbolic root in breadcrumbs), dirty dot `●`, per-tab mode persistence, **Line Wrap / Horizontal Scroll** (`WrapText`), `[Save]` (`Ctrl+S` / `Cmd+S` → `save_project_document`), and `[Discard]`.
- **Language grammars & real-time linting:**
  - Lezer parsers (`rust`, `javascript`/`typescript`, `python`, `cpp`, `html`, `css`, `json`, `markdown`), SFC parsers (`vue`, `svelte`, `astro`), and legacy stream modes (`toml`, `shell`, `yaml`, `go`, `dockerfile`, `sql`, `xml`, `ruby`, `properties`, `diff`).
  - `@codemirror/lint` (`lintGutter` + `linter` using full-document `language.parser.parse(text)` fallback when incremental parsing is incomplete, strict `JSON.parse`, and offset deduplication), reporting `N SYNTAX ERR` in the editor footer.
- **Docked side drawers & context menu:**
  - Inline side drawers (Outline `Ctrl+Shift+O` via `get_file_symbols`, Graph Insights with inline chunk/file preview, and Comments) render side-by-side without obscuring code.
  - Right-click `AppContextMenu` triggers **Ask Agent**, **Refactor with Agent**, **Add Review Comment**, **Graph Insights**, and **Go to Definition** (`F12` / `Ctrl+Click` via `resolve_symbol_definition`).

## Contracts

### Daemon Commands (`api.rs` & `kopeng/documents.rs` / `semantic/mod.rs`)

- **Documents & Explorer:** `get_project_file_tree`, `get_project_documents`, `read_project_document`, `save_project_document`, `create_project_directory`, `create_project_file`, `rename_project_item`, `delete_project_items`, `copy_project_items`, `move_project_items`, `save_clipboard_image`, `attach_file_to_project`, `paste_clipboard_image`, `open_file_in_external_editor`, `get_project_exclusion_status`, `toggle_project_exclusion`
- **Comments:** `create_document_comment`, `list_document_comments`, `resolve_document_comment`, `delete_document_comment`
- **Symbols:** `get_file_symbols`, `resolve_symbol_definition`

### Events, MCP & Config

- **SSE Events:** `project-files-changed`, `DocumentCommentResolved`, `conversation-topic-switched`
- **MCP Tools:** `list_document_comments`, `resolve_document_comment`, `switch_conversation_topic`
- **Config Keys:** `vcs.scratchpad_retention_days` (default `0`, disabled)

## Implementation Status

| Area | Status | Notes |
| :--- | :--- | :--- |
| `docs/` & `scratchpad/` file commands, path sandboxing, `notify` watcher, `scratchpad_retention_days` (`kopeng/documents.rs`, `config.rs`) | Implemented | — |
| `document_comments` table, commands, SSE event, and MCP tools (`db.rs`, `documents.rs`, `mcp.rs`, `useDocumentComments.ts`) | Implemented | — |
| `agent_conversations`, `agent_chat_turns`, `switch_conversation_topic`, 60-line terminal snapshot (`conversation.rs`, `db.rs`) | Implemented | — |
| CodeMirror 6 editor (`AppCodeEditor.vue`), linting, Outline (`get_file_symbols`), Go to Definition (`resolve_symbol_definition`) | Implemented | — |
| Reader, Explorer multi-select/clipboard, Comments drawer, Three-Tier artifact switchers | Implemented | Consolidated directly in `ProjectWorkspace.vue` and `FloatingChatOverlay.vue` rather than standalone `ArtifactPanel.vue` / `DocTreeNode.vue` files |

## Related Specs

- [Project Registry, Scripts & Service Orchestration](01-project-registry-and-service-orchestration.md)
- [Greenfield Project Creation](10-greenfield-project-creation.md)
- [Native Gemini Dictation](../05-voice/02-dictation-and-agent-chat.md)
- [Design System Foundations](../07-ui/01-design-system-foundations.md)
- [Project Category Index](README.md)

Supersedes legacy: `20_live_artifacts_and_project_scratchpad.md`
