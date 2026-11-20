# Session Lifecycle, Database Devlog & Knowledge Graph

## Purpose

Establish a unified, database-backed foundation for agent session continuity, project history, actionable backlog tracking, and transversal knowledge discovery—replacing fragmented chat topics and flat Markdown files (`devlog/`, `TODO.md`, `TECHNICAL_DEBT.md`) with relational SQLite ledgers, deterministic AST/Git provenance, and local semantic vector search.

## Scope

In scope:
- The tri-state agent session lifecycle taxonomy (`unengaged_draft`, `trampoline`, `promoted`) and continuous conversational feed ergonomics.
- Relational SQLite schemas for session minutes (`agent_session_journal`), tasks (`project_todos`), technical debt (`project_technical_debt`), initialization flags (`project_initialization_flags`), causal knowledge edges (`knowledge_edges`), Git diff hunks (`journal_file_hunks`), and Tree-sitter AST symbols (`project_code_symbols`).
- MCP JSON-RPC 2.0 tools and daemon API commands for recording minutes, managing tasks/debt/flags, searching knowledge, and resolving file-line insights.
- Transversal semantic search architecture (LanceDB `knowledge_v2`, local `BAAI/bge-m3` via ONNX Runtime, adaptive idle throttler, multi-language AST/Markdown/PDF chunking, two-tier SHA-256 incremental indexing, reactive auto-indexing, and CLI audit endpoint).
- Omnisearch (`Ctrl + K`), Project Overview knowledge card, and the interactive `Graph` workspace tab (`ProjectKnowledgeGraphTab.vue`).

Out of scope:
- Conversation thread storage internals and Brief generation mechanics (covered in the conversation continuity specification).
- Attested MCP caller resolution and HTTP/SSE daemon transport internals (covered in the Streamable HTTP MCP daemon specification).
- Project milestone planning and Kanban task workflows (covered in the project management specification).

## Model / Design

### 1. Motivation & Unified Knowledge Graph

Historically, project history and conversational state were fragmented across three disconnected patterns:
1. **Siloed Chat Sessions:** Topic dropdowns (`+ New Topic`) discarded underlying native session threads (`agy_conversation_id`) and fragmented agent dialogue.
2. **Static Markdown Sprawl:** Flat `devlog/`, `TODO.md`, and `TECHNICAL_DEBT.md` files were invisible to relational queries, expensive to dump into agent context windows, and disconnected from the execution cycles that produced them.
3. **Session Waste & Startup Overhead:** Spawning a fresh CLI session on every agent start or restart forced rule and file re-reading even before any user interaction occurred.

ONE-SHIP unifies these into five pillars:
- **Continuous Conversational Stream:** Each agent maintains one unbroken chronological timeline; context switches render as inline milestone separators rather than separate windows.
- **Tri-State Session Lifecycle:** Native CLI sessions are classified as `Idle Pool`, `Trampoline`, or `Substantive` to eliminate startup token waste and prevent transient routing sessions from polluting project history.
- **First-Class Database Entities:** Journal minutes, tasks, and technical debt live in structured SQLite tables populated by agents via MCP tools.
- **Development Knowledge Graph:** Connects specifications (`docs/specs/`), architectural decisions, touched files/hunks, tasks, technical debt, and code symbols to the session history that produced them.
- **Transversal Semantic Search:** Decouples vector indexing and AST symbol extraction into a platform-wide daemon service accessible to both agents (MCP) and developers (Omnisearch and Graph UI).

```text
┌─────────────────────────────────────────────────────────────────────────────┐
│                          THE DEVELOPMENT KNOWLEDGE GRAPH                    │
│                                                                             │
│   docs/specs/ ◄──────────┐                                                  │
│   (Requirements)         │                                                  │
│                          ▼                                                  │
│               ┌──────────────────────┐         ┌──────────────────────┐     │
│               │ agent_session_journal│◄────────┤    project_todos     │     │
│               │ (Database Devlog)    │         │ (Actionable Tasks)   │     │
│               └──────────┬───────────┘         └──────────────────────┘     │
│                          │                                                  │
│                          ├─────────────────────►┌──────────────────────┐     │
│                          │                      │project_technical_debt│     │
│                          ▼                      │(Bottlenecks/Defects) │     │
│               ┌──────────────────────┐          └──────────────────────┘     │
│               │   agy_session_uuid   │                                      │
│               │(Antigravity Brain Id)│◄─────────┐                           │
│               └──────────┬───────────┘          │                           │
│                          │                      │                           │
│                          ▼                      │                           │
│               ┌──────────────────────┐          │                           │
│               │  agent_chat_turns    ├──────────┘                           │
│               │ (Continuous Stream)  │                                      │
│               └──────────────────────┘                                      │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 2. Tri-State Session Lifecycle Taxonomy

Native agent CLI sessions spawned by the process supervisor progress through three lifecycle classifications:

```text
┌─────────────────────────────────────────────────────────────────────────────┐
│                     TRI-STATE AGY SESSION LIFECYCLE                         │
└──────────────────────────────────────┬──────────────────────────────────────┘
                                       │
                                       ▼
                    [Supervisor Spawns Agent Process]
                                       │
                                       ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ 1. IDLE POOL SESSION (`unengaged_draft`)                                    │
│    - Spawned with initial persona directives & ready handshake.             │
│    - Contains 0 user messages (`user_turns == 0`).                          │
│    - Retained in cache per agent. On app restart, this session is REUSED    │
│      via `agy --conversation <uuid>` to prevent token re-reading overhead.  │
└──────────────────────────────────────┬──────────────────────────────────────┘
                                       │
                    What occurs during execution?
                                       │
        ┌──────────────────────────────┴──────────────────────────────┐
        ▼                                                             ▼
[Used solely for discovery/routing]               [Substantive work & decisions]
        │                                                             │
        ▼                                                             ▼
┌──────────────────────────────────────┐     ┌──────────────────────────────────────┐
│ 2. TRAMPOLINE SESSION (`trampoline`) │     │ 3. SUBSTANTIVE SESSION (`promoted`)  │
│    - User asked to switch topics.    │     │    - Real implementation occurred.   │
│    - Agent invoked search & jumped.  │     │    - Agent invokes MCP tool:         │
│    - No original work generated.     │     │      `record_session_minutes()`      │
│    - Pruned upon successful handoff. │     │    - Formally indexed in database.   │
│    - Never enters devlog or graph.   │     │    - Becomes node in Knowledge Graph.│
└──────────────────────────────────────┘     └──────────────────────────────────────┘
```

1. **Idle Pool Sessions (`unengaged_draft`):** When the supervisor spawns an agent, the CLI creates a native session UUID. If the agent completes its ready handshake without receiving a user prompt (`user_turns == 0`), restarting the app or agent inspects the latest session and resumes `agy --conversation <uuid>` rather than allocating a new UUID and re-reading project rules.
2. **Trampoline Sessions (`trampoline`):** If a user opens an idle session solely to resume an earlier topic, the agent invokes `search_conversations` to locate the target ONE-SHIP Conversation and `reopen_conversation` to continue it from a Brief. Because the routing session generated no original technical decisions, it is silently pruned from the turn ledger upon handoff.
3. **Substantive Sessions (`promoted`):** Invoking `record_session_minutes()` promotes the work into the persistent historical ledger, triggers embedding/graph updates, records touched files and Git hunks, and links the entry into the Development Knowledge Graph.

### 3. Project-First Relational State (`oneship.db`)

All structured project history resides in `~/.config/one-ship/oneship.db`.

> **Project-First Lifecycle Invariant (Zero Agent Coupling):**
> Development logs, active tasks, and technical debt represent the chronological lifecycle of the *project as a whole*, not an isolated agent silo.
> 1. Primary hydration queries retrieve project-wide history (`agent_id = None`), enabling inspection when no agent subprocess is open or active.
> 2. Agent filtering in the UI is strictly an optional display facet and never restricts the primary data pipeline.
> 3. Conversation reopening (`reopen_conversation`) binds dynamically to the record's originating `agent_id` rather than transient UI focus.
> 4. The Journal panel exposes the three record families as `Devlogs | Tasks | Tech Debt` sub-tabs sharing one search box and one filter-chip row; filters are display-only and never alter stored records.

### 4. Transversal Semantic Search & Causal Graph Pipeline

Semantic search is a platform-wide daemon service rather than an isolated chat utility.

#### 4.1 Hybrid Storage & Scope Separation
1. **Prose Vector Embeddings (LanceDB `knowledge_v2`):**
   - Stored in `~/.config/one-ship/vectors.lance/` using the in-process `lancedb` crate (Apache Arrow columnar memory + Lance disk format), partitioned by `project_id` with native scalar filtering + 1024-dimension Vector ANN search.
   - Table `knowledge_v2` automatically drops any legacy `knowledge` table on upgrade.
   - **Prose-only policy:** Indexes architectural specifications/documentation (`specs`: `.md`, `.txt`, `.pdf`), devlog session minutes (`journal`), tracked TODOs (`tasks`), and technical debt items (`tech_debt`). Source code and static config/HTML data files are excluded from dense vectorization to eliminate cross-domain cosine similarity noise.
2. **Local Embedding Engine (`BAAI/bge-m3`):**
   - Executed locally via ONNX Runtime (`fastembed` crate), producing 1024-dimension dense vectors across an **8,192-token** context window with native multilingual support (English, Spanish, and programming syntax).
3. **Deterministic AST Symbols (`project_code_symbols` in SQLite):**
   - Multi-language Tree-sitter symbol declarations (`file_path`, `symbol_name`, `short_name`, `line_start`, `line_end`) indexed incrementally via `file_sha256` with zero ONNX inference.
4. **Causal Git Hunks (`journal_file_hunks` in SQLite):**
   - Line-level diff ranges (`line_start`, `line_end`, `change_kind`, `source_kind`) extracted deterministically from `record_session_minutes` commits (`git show --unified=0`) or snapshots (`git diff --unified=0`), falling back to `files_touched` when VCS checkpoints are off or during historical backfill.

#### 4.2 Adaptive Idle Throttler
Background indexing is governed by `sysinfo` host telemetry and native supervisor state:
- **Supervisor Task Concurrency Gate:** Pauses immediately whenever ONE-SHIP is executing internal project tasks (`RunTargetType::Task` in `Working` state: compilation, test suites, migrations). External OS processes outside ONE-SHIP are never probed or assumed.
- **Host RAM & CPU Gates:** Checks `semantic.throttler_min_free_mem_mbytes` (default `2048` MiB / 2.0 GiB) and `semantic.throttler_max_cpu_percent` (default `45%`).
- **Worker Priority & Cooperative Yielding:** Background runs (`force = false`) execute on an isolated OS thread at `nice 19` with constrained ONNX intra-op threads (`semantic.onnx_intra_threads_count`, default `2`), processing micro-batches (`semantic.embedding_batch_size_count`, default `10`; design target 5–10 chunks) with `tokio::task::yield_now().await` and `semantic.embedding_batch_sleep_ms` (default `20` ms).
- **Manual / Interactive Exemption:** User-facing queries and explicit manual re-indexing (`force = true`) bypass idle CPU/task throttling and enforce only the critical OOM safety floor (`semantic.throttler_critical_free_mem_mbytes`, default `512` MiB).

#### 4.3 Multi-Language Structural & AST Chunking Pipeline
1. **Universal Recursive Code Traversal:** Discovers files from `root_path` (supporting monorepos and nested layouts such as `backend/app/**/*.py`, `frontend/src/**/*.{ts,tsx}`, `connectors/**/manifest.toml`). Automatically skips `docs/`, `specs/`, hidden directories (`.*`), and paths matching `ignored_patterns`.
   - **Tier 1 (Declarative Tree-sitter AST Parsing):** S-expression queries map declarations to canonical node categories (`function`, `method`, `class`, `struct`, `enum`, `trait`, `interface`, `impl`, `module`, `import`, `call`) across **Rust, TypeScript/TSX, JavaScript/JSX, Python, Go, Java, C#, C, C++, Ruby, PHP, Bash, HTML, CSS, SQL, TOML, YAML, JSON**. Single-file components (`.vue`, `.svelte`, `.astro`) extract `<script>`/`<script setup>` and `<style>` blocks and delegate to Tier 1 grammars. Leading docstrings/doc-comments are attached to symbol chunks.
   - **Stable Symbol Identity Key:** `<rel_path>::<qualified_symbol>` (with deterministic `#<occurrence>` suffix for overloads/repeats), intentionally omitting line numbers from `id` so line shifts do not orphan LanceDB vectors.
   - **Tier 2 (Language-Agnostic Structural Fallback):** Languages without a Tier 1 grammar (e.g., Zig, Nix, Lua, Elixir, Haskell, Verilog) use a brace-depth `{}` and indentation-aware block segmenter bounded by blank-line transitions, falling back to overlapping sliding windows (`40` lines per block, `10`-line overlap) only when no structural blocks exist.
2. **Living Specifications & Documentation (`.md`, `.txt`, `.pdf`):**
   - **Markdown:** Parsed into `H1`–`H6` hierarchy (ignoring `#` inside fenced ```` ``` ```` / `~~~` blocks) with **dynamic bottom-up coalescing**: `H4`–`H6` sub-headings remain coalesced inside parent `H1`–`H3` sections when the combined block fits within `4,000` bytes; larger sections subdivide along `H4`–`H6` while coalescing consecutive small siblings `< 1,200` bytes and preserving full ancestor breadcrumbs (`H1 > H2 > ...`) in `title`. Zero artificial byte truncation is applied so BGE-M3 uses its full 8,192-token window.
   - **PDF:** Extracted via pure-Rust PDF text extraction (`pdf-extract` / `lopdf`) and segmented into `file_path#page_<n>` chunks.
3. **Database Entities:** Segmented atomically from SQLite using identity keys `journal::<session_id>`, `todo::<item_id>`, and `debt::<item_id>`.

#### 4.4 Two-Tier SHA-256 Incremental Indexing & Live Status
1. **Schema Support:** LanceDB stores `file_sha256` (`Utf8`) and `content_sha256` (`Utf8`) alongside embeddings and line offsets (pre-hash tables missing these columns are cleanly recreated).
2. **Level 1 (File-Level Hash):** Fetches `(id, file_path, file_sha256, content_sha256, line_start, line_end)` per `project_id` in one query; files whose `file_sha256` matches all stored chunks skip parsing and embedding.
3. **Level 2 (Symbol/Block Diffing & Orphan Pruning):** Unchanged chunks (`id` + `content_sha256` match) reuse stored vectors and update only shifted line numbers / `file_sha256`; modified or new chunks are embedded via BGE-M3; removed files/symbols are deleted in bounded batch `DELETE` statements (`id IN (...)`).
4. **Live Runtime Registry & JIT Line Resolution:** An in-memory poison-safe registry (`OnceLock<RwLock<HashMap<i64, IndexingRuntimeState>>>`) tracks `is_indexing` and `last_duration_ms` and deduplicates concurrent runs per `project_id`. Search results resolve `file_path` against the owning project's `root_path` before verifying live line offsets on disk.

#### 4.5 Causal Knowledge Graph (`knowledge_edges`) & Reactive Auto-Indexing
To provide high-signal causal traceability without noisy AST `calls` or doc-to-code cosine matching, `knowledge_edges` combines deterministic Git/SQLite provenance with prose-only semantic links:
1. **Causal & Exact-Path Edges (`weight = 1.0`):**
   - `touched` (`origin = "git_hunk"` | `"files_touched"` | `"explicit_ref"`): `journal -> file` derived from `journal_file_hunks`.
   - `affects` (`origin = "anchor"` | `"explicit_ref"`): `tech_debt / tasks -> file` derived from optional `file_path`/`line_start`/`line_end` anchors or verified workspace relative paths in prose.
   - `relates_to` (`origin = "conversation"`): `tech_debt / tasks -> journal` via optional `journal_id`.
   - `governs` / `references` / `documents` (`origin = "explicit_ref"`): verified workspace relative paths cited in specs or minutes.
2. **Prose-Only Semantic Edges (`origin = "semantic"`):**
   - `documents` (`journal -> specs`, capped to top 5 per minute) and `relates_to` (`tech_debt / tasks -> journal`, capped to top 3 per item) computed via BGE-M3 cosine similarity when `>= semantic.semantic_edge_min_score_percent` (default `70`).
   - `rebuild_project_knowledge_graph` recomputes `knowledge_edges` in `< 300 ms` directly from stored LanceDB 1024-dim prose vectors, `journal_file_hunks`, SQLite anchors, and exact path references without re-running BGE-M3 inference.
3. **Reactive Background Auto-Indexing:**
   - Workspace file edits (`documents.rs` `notify` watcher) invoke `schedule_background_indexing_for_path` debounced by `semantic.file_watch_debounce_ms` (default `10000` ms).
   - Database entity mutations (`record_session_minutes`, `manage_todo_item`, `resolve_todo_item`, `record_technical_debt`, `resolve_technical_debt`) invoke `schedule_background_indexing_for_entity` debounced by `semantic.entity_watch_debounce_ms` (default `2000` ms).
   - Background auto-indexing runs only for projects already initialized (`total_records_count > 0 || code_symbols > 0`) when `[plugins.semantic].enabled == true`.

#### 4.6 UI & Conversational Ergonomics
1. **Continuous Chronological Feed:** Eliminates manual topic dropdowns and creation popovers. Conversation transitions between consecutive turns render inline separators keyed on ONE-SHIP `conversation_id` (not runner session IDs), with a `Reopen` action targeting the Conversation above (`reopen_conversation`).
2. **Lazy Hydration & Pagination:** Hydrates the latest **5 messages** by default (`limit: 5`); `[↑ Load older messages (+10)]` paginates backward via `before_id` cursor without scroll jumps.
3. **Column 3 Knowledge Modes:** `[Live Artifact]`, `[Docs & Scratchpad]`, `[Devlog Journal]`, and `[Terminal Log]`.
4. **Omnisearch (`Ctrl + K` & `AppHeader` search button):** Dispatches directly to `daemonCall("search_knowledge", ...)` without LLM involvement, rendering results in `< 30 ms` with `[CODE]`, `[SPECS]`, `[JOURNAL]`, `[TASKS]`, `[DEBT]` category tags and clickable 1-hop `Graph:` relation chips that jump straight to editor line ranges or database records.
5. **Project Overview Subsystems Card (`OverviewWidgetGrid.vue`):** Displays total `CHUNKS`, total `GRAPH EDGES`, category breakdown (`SYMBOLS`, `SPECS`, `ENTITIES`), `AUTO-SYNC: ON/OFF`, active `MIN <n>%` threshold, last run duration, and an `Index` button.
6. **Interactive Knowledge Graph Tab (`id: 'knowledge'`, Label: `Graph`, `ProjectKnowledgeGraphTab.vue`):**
   - D3-powered (`d3-force`, `d3-zoom`, `d3-drag`, `d3-selection`, `d3-transition`) spatial causal graph across `Specs`, `Journal / Tasks / Tech Debt`, and `Files`.
   - Expandable Spec super-nodes (`+` / `-` or double-click) revealing Markdown sections with structural `contains` links.
   - Quadratic Bezier directional edges with `<marker>` arrowheads, bidirectional arc separation, animated flow on selection/hover, and midpoint weight pills (`touched` in accent, `documents`/`governs` in status-ok, `affects`/`relates_to` in status-warn).
   - `120`-tick pre-warmed equilibrium, `Stratified Flow` (`Specs` left -> `Minutes / Debt` middle -> `Files` right) and organic `Force` layouts, `Fit`/`+`/`-` zoom controls, drag-to-pin, explicit-submit semantic spotlight (`Enter` or Search button), live `Min Weight` slider (`50%..100%`), `< 300 ms` `Rebuild`, and resizable right inspector with `All / 1-Hop / 2-Hop` Ego-Graph isolation.
7. **Explorer Editor Causal Insights & Navigation:** `get_file_line_insights` populates the Causal Insights drawer (`minutes` with commit/snapshot diff links, `debt_and_todos`, `related_docs`); `get_file_line_range_content` renders inline bounded previews without switching tabs; `get_file_symbols` populates the Outline tree; `resolve_symbol_definition` powers `F12` / `Ctrl+Click` Go to Definition.

## Contracts

### 1. SQLite Schemas (`~/.config/one-ship/oneship.db`)

#### `agent_session_journal`
Replaces `devlog/YYYY-MM-DD_<topic>.md`. Keyed by the ONE-SHIP Conversation resolved from the attested caller; `conversation_id` is `NULL` only for rows predating the Conversation key migration (existing databases are rebuilt once in place, preserving rows).

```sql
CREATE TABLE IF NOT EXISTS agent_session_journal (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    project_id INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    agent_id TEXT NOT NULL,
    conversation_id TEXT REFERENCES agent_conversations(id) ON DELETE SET NULL,
    agy_conversation_id TEXT,
    title TEXT NOT NULL,
    summary TEXT NOT NULL,
    decisions TEXT,          -- JSON Array: ["Decided X over Y", "Enforced rule Z"]
    files_touched TEXT,      -- JSON Array: ["src-tauri/src/db.rs", "src/App.vue"]
    technical_debt TEXT,     -- JSON Array: Debt items resolved or created
    tags TEXT,               -- JSON Array: ["persistence", "sqlite", "ui"]
    context_pct REAL,
    total_tokens INTEGER,
    milestone_id INTEGER REFERENCES project_milestones(id) ON DELETE SET NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    embedding BLOB
);

CREATE INDEX IF NOT EXISTS idx_session_journal_proj ON agent_session_journal(project_id, agent_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_session_journal_agy ON agent_session_journal(agy_conversation_id);
CREATE INDEX IF NOT EXISTS idx_session_journal_conv ON agent_session_journal(conversation_id, id DESC);
CREATE INDEX IF NOT EXISTS idx_session_journal_milestone ON agent_session_journal(project_id, milestone_id);
```

*(Note: `milestone_id` is added idempotently via `ensure_column` on startup; `record_session_minutes` also tracks optional VCS checkpoint hashes when `[plugins.vcs].minutes_checkpoint_mode` is `"commit"` or `"snapshot"`.)*

#### `project_todos`
Replaces static `TODO.md` entries (`conversation_id` added via `ensure_column`):

```sql
CREATE TABLE IF NOT EXISTS project_todos (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    project_id INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    agent_id TEXT,
    agy_conversation_id TEXT,
    title TEXT NOT NULL,
    description TEXT,
    category TEXT NOT NULL DEFAULT 'feature', -- 'feature', 'bugfix', 'refactor', 'docs'
    status TEXT NOT NULL DEFAULT 'pending',   -- 'pending', 'in_progress', 'completed', 'cancelled'
    priority INTEGER NOT NULL DEFAULT 2,      -- 1: high, 2: medium, 3: low
    created_at INTEGER NOT NULL,
    completed_at INTEGER
);

CREATE INDEX IF NOT EXISTS idx_todos_project_status ON project_todos(project_id, status, priority);
```

#### `project_technical_debt`
Replaces static `TECHNICAL_DEBT.md` entries (`conversation_id` and `milestone_id` added via `ensure_column`):

```sql
CREATE TABLE IF NOT EXISTS project_technical_debt (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    project_id INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    agy_conversation_id TEXT,
    title TEXT NOT NULL,
    category TEXT NOT NULL DEFAULT 'architectural', -- 'architectural', 'performance', 'maintainability'
    impact_summary TEXT NOT NULL,
    remediation_plan TEXT,
    status TEXT NOT NULL DEFAULT 'open',      -- 'open', 'in_progress', 'resolved'
    severity TEXT NOT NULL DEFAULT 'medium',  -- 'critical', 'high', 'medium', 'low'
    milestone_id INTEGER REFERENCES project_milestones(id) ON DELETE SET NULL,
    identified_at INTEGER NOT NULL,
    resolved_at INTEGER
);

CREATE INDEX IF NOT EXISTS idx_tech_debt_proj ON project_technical_debt(project_id, status, category);
CREATE INDEX IF NOT EXISTS idx_tech_debt_milestone ON project_technical_debt(project_id, milestone_id);
```

#### `project_initialization_flags`
Tracks decoupled onboarding gaps discovered in Phase 1 requiring independent Phase 2 resolution:

```sql
CREATE TABLE IF NOT EXISTS project_initialization_flags (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    project_id INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    flag_type TEXT NOT NULL,                  -- 'docs', 'todos', 'tech_debt', 'profiler', 'invalid_config', 'removal'
    title TEXT NOT NULL,
    description TEXT,
    recommended_option TEXT,
    options_json TEXT,
    metadata_json TEXT,
    status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending', 'resolved', 'dismissed')),
    raised_by_agent_uid TEXT,
    resolved_by_agent_uid TEXT,
    created_at INTEGER NOT NULL,
    resolved_at INTEGER
);

CREATE INDEX IF NOT EXISTS idx_init_flags_proj ON project_initialization_flags(project_id, status);
```

#### `knowledge_edges`, `journal_file_hunks`, and `project_code_symbols`

```sql
CREATE TABLE IF NOT EXISTS knowledge_edges (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    project_id INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    source_id TEXT NOT NULL,
    source_entity_type TEXT NOT NULL,
    source_title TEXT NOT NULL,
    source_file_path TEXT,
    source_line_start INTEGER,
    target_id TEXT NOT NULL,
    target_entity_type TEXT NOT NULL,
    target_title TEXT NOT NULL,
    target_file_path TEXT,
    target_line_start INTEGER,
    edge_type TEXT NOT NULL,
    origin TEXT NOT NULL DEFAULT 'explicit_ref',
    weight REAL NOT NULL DEFAULT 1.0,
    updated_at_epoch_seconds INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_knowledge_edges_source ON knowledge_edges(project_id, source_id);
CREATE INDEX IF NOT EXISTS idx_knowledge_edges_target ON knowledge_edges(project_id, target_id);

CREATE TABLE IF NOT EXISTS journal_file_hunks (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    project_id INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    journal_id INTEGER NOT NULL REFERENCES agent_session_journal(id) ON DELETE CASCADE,
    file_path TEXT NOT NULL,
    line_start INTEGER NOT NULL DEFAULT 0,
    line_end INTEGER NOT NULL DEFAULT 0,
    change_kind TEXT NOT NULL DEFAULT 'modified',
    source_kind TEXT NOT NULL DEFAULT 'git_commit',
    created_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_journal_file_hunks_lookup
    ON journal_file_hunks(project_id, file_path, line_start, line_end);
CREATE INDEX IF NOT EXISTS idx_journal_file_hunks_journal
    ON journal_file_hunks(journal_id);

CREATE TABLE IF NOT EXISTS project_code_symbols (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    project_id INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    file_path TEXT NOT NULL,
    file_sha256 TEXT NOT NULL,
    symbol_name TEXT NOT NULL,
    short_name TEXT NOT NULL,
    title TEXT NOT NULL,
    line_start INTEGER NOT NULL,
    line_end INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_project_code_symbols_proj_file
    ON project_code_symbols(project_id, file_path);
CREATE INDEX IF NOT EXISTS idx_project_code_symbols_proj_short
    ON project_code_symbols(project_id, short_name);
```

### 2. MCP JSON-RPC 2.0 Tool Primitives (Port `8765`)

- **`record_session_minutes`:** Promotes the session to `agent_session_journal` and triggers `[plugins.vcs].minutes_checkpoint_mode` (`snapshot`, `commit`, or `off`). Rejected with `isError: true` if the caller has a pending `delegate_agent_task` assignment (must use `report_task_status`) or if `minutes_checkpoint_mode = "commit"` fails validation/Git hooks (journal row is rolled back and hook output is returned). Keyed by attested ONE-SHIP Conversation (no identifier parameter accepted).

```json
{
  "name": "record_session_minutes",
  "description": "Records structured meeting minutes and technical decisions for the active session, promoting it to the permanent devlog journal.",
  "parameters": {
    "type": "object",
    "properties": {
      "title": { "type": "string", "description": "Concise summary title of the milestone or topic" },
      "summary": { "type": "string", "description": "Executive overview of changes and progress" },
      "conversation_summary": { "type": "string", "description": "Optional. Summary of the whole current Conversation; replaces the previous one and seeds future Briefs" },
      "decisions": { "type": "array", "items": { "type": "string" }, "description": "Key technical trade-offs or decisions made" },
      "files_touched": { "type": "array", "items": { "type": "string" }, "description": "Relative paths to modified files" },
      "technical_debt": { "type": "array", "items": { "type": "string" }, "description": "Any technical debt incurred or resolved" },
      "tags": { "type": "array", "items": { "type": "string" } }
    },
    "required": ["title", "summary"]
  }
}
```

- **`query_session_journal`:**

```json
{
  "name": "query_session_journal",
  "description": "Queries historical development journal entries and session minutes for a project.",
  "parameters": {
    "type": "object",
    "properties": {
      "project": { "type": "string", "description": "Optional project id, path or name." },
      "agent_id": { "type": "string", "description": "Optional agent ID filter." },
      "limit": { "type": "number", "description": "Max entries to return (default: 30)." }
    }
  }
}
```

- **`search_conversations` & `reopen_conversation`:** Act on the caller's attested project and agent (no identity parameters). `reopen_conversation` closes the current Conversation, opens a new one linked by `reopened_from`, and returns the reopened Conversation's Brief.

```json
{ "name": "search_conversations", "parameters": { "query": "string", "status": "active | archived (optional)" } }
{ "name": "reopen_conversation", "parameters": { "conversation_id": "string (from search_conversations)" } }
```

- **`manage_todo_item`:**

```json
{
  "name": "manage_todo_item",
  "description": "Lists, creates, updates, or completes active project tasks in the database backlog.",
  "parameters": {
    "type": "object",
    "properties": {
      "action": { "type": "string", "enum": ["list", "create", "update_status"] },
      "todo_id": { "type": "integer" },
      "title": { "type": "string" },
      "status": { "type": "string", "enum": ["pending", "in_progress", "completed", "cancelled"] },
      "priority": { "type": "integer" }
    },
    "required": ["action"]
  }
}
```

- **`record_technical_debt`:**

```json
{
  "name": "record_technical_debt",
  "description": "Records architectural bottlenecks or tracks remediation progress in the database technical debt ledger.",
  "parameters": {
    "type": "object",
    "properties": {
      "action": { "type": "string", "enum": ["list", "record", "resolve"] },
      "debt_id": { "type": "integer" },
      "title": { "type": "string" },
      "category": { "type": "string", "enum": ["architectural", "performance", "maintainability"] },
      "impact_summary": { "type": "string" },
      "severity": { "type": "string", "enum": ["critical", "high", "medium", "low"] }
    },
    "required": ["action"]
  }
}
```

- **`raise_initialization_flag` & `resolve_initialization_flag`:**

```json
{
  "name": "raise_initialization_flag",
  "description": "Emits an initialization flag to signal an uncompleted bootstrapping domain (docs, todos, tech_debt, profiler) to be resolved in Phase 2.",
  "parameters": {
    "type": "object",
    "properties": {
      "flag_type": { "type": "string", "enum": ["docs", "todos", "tech_debt", "profiler"] },
      "title": { "type": "string" },
      "description": { "type": "string" },
      "metadata_json": { "type": "string" }
    },
    "required": ["flag_type", "title"]
  }
}
```

```json
{
  "name": "resolve_initialization_flag",
  "description": "Marks an initialization flag as resolved or dismissed after completing its dedicated remediation session.",
  "parameters": {
    "type": "object",
    "properties": {
      "flag_id": { "type": "integer" },
      "status": { "type": "string", "enum": ["resolved", "dismissed"] }
    },
    "required": ["flag_id"]
  }
}
```

- **`search_knowledge`:** Parameters: `query` (`string`), `scope` (`"all" | "code" | "specs" | "journal" | "tasks" | "tech_debt"`, default `"all"`), `limit` (`integer`, default `5`), `include_relations` (`boolean`, default `true`), `max_relations_per_result` (`integer`, optional), `min_relation_score_percent` (`integer` `1–100`, optional).
- **`get_file_line_insights`:** Deterministic (non-vector) causal lookup returning enclosing AST/Markdown symbol, `minutes`, `debt_and_todos`, and `related_docs` for `(rel_path, line)`.

### 3. Daemon API Commands, Capability Core & Audit Endpoint

- **Knowledge & Graph Daemon Commands (`api::API_COMMANDS` / `src/types/daemonApi.ts`):**
  - `search_knowledge`: Hybrid search (LanceDB prose vectors + `project_code_symbols` AST symbols) with optional 1-hop `knowledge_edges` expansion.
  - `index_project_knowledge`: Incremental prose vectorization, deterministic SQLite AST symbol sync, historical `journal_file_hunks` backfill, and causal graph rebuild.
  - `get_indexing_status`: Live vector count, `codeChunksCount`, `is_indexing`, and `last_duration_ms`.
  - `get_project_knowledge_graph`: Returns `ProjectKnowledgeGraph` (prose nodes, synthesized touched-`file` nodes, and causal/prose edges).
  - `rebuild_project_knowledge_graph`: Recomputes `knowledge_edges` in `< 300 ms` without BGE-M3 inference.
  - `get_file_line_insights`: Returns `FileLineInsights` for `(project_id, rel_path, line)`.
  - `get_file_line_range_content`: Returns `FileLineRangeContent` for inline drawer preview.
  - `get_file_symbols`: Returns `FileSymbolItem[]` (Tree-sitter symbols or Markdown headings with `level`) for the Outline tree.
  - `resolve_symbol_definition`: Cross-project deterministic lookup against `project_code_symbols` returning `SymbolDefinitionLocation[]` for `F12` / `Ctrl+Click`.
- **Journal, Task & Debt Daemon Commands:** `get_project_session_journals`, `get_project_todos`, `create_project_todo`, `update_todo_status`, `get_project_technical_debt`, `record_technical_debt`, `resolve_technical_debt`, `search_conversations`, `reopen_conversation`.
- **Capability Core (`domain = "knowledge"`):** Exposes `search_knowledge`, `get_file_line_insights`, `resolve_symbol_definition`, `get_file_symbols`, `get_indexing_status`, `index_project`, and `rebuild_graph`.
- **External CLI Audit Endpoint:** `POST /api/v1/audit/semantic` served on `127.0.0.1` by the active daemon and invoked via `./scripts/audit_semantic.sh --project-id <id> [--reindex] [--candidate-min 60] [--query "..."]` without spawning a second ONNX Runtime or LanceDB instance.

### 4. Configuration Keys (`[plugins.semantic]`)

| Key | Type | Scope | Default | Purpose |
| :--- | :--- | :--- | :--- | :--- |
| `enabled` | `boolean` | `cascade` | `true` | Enables/disables background indexing and semantic search. |
| `graph_tab_enabled` | `boolean` | `cascade` | `true` | Shows/hides the `Graph` workspace tab (`ProjectKnowledgeGraphTab.vue`). |
| `ignored_patterns` | `string_list` | `cascade` | `["target", "node_modules", "dist", "build", ".git", ".env*", "*.min.js", "*.lock", "__pycache__", ".venv", "venv", "*.pyc", "scratchpad"]` | Excluded patterns distinct from `.gitignore` so local research/docs stay indexed. |
| `included_extensions` | `string_list` | `cascade` | `["rs", "ts", "tsx", "js", "jsx", "py", "vue", "svelte", "astro", "go", "java", "cs", "c", "cpp", "h", "hpp", "rb", "php", "css", "sql", "sh", "md", "txt", "pdf"]` | Extensions processed for AST symbol extraction and prose parsing (`.html`/`.toml`/`.yaml`/`.yml`/`.json` excluded from code symbol chunking). |
| `extra_paths` | `string_list` | `cascade` | `["docs", "specs", "devlog", "research", "manuals"]` | Project subdirectories recursively scanned for documentation and specs. |
| `semantic_edge_min_score_percent` | `number` | `cascade` | `70` | Minimum cosine similarity percentage (`1–100`) for `origin = "semantic"` graph edges. |
| `max_file_size_bytes` | `number` | `cascade` | `262144` | Max file size in bytes (`256 KiB`; `0` disables) for code and text files. |
| `file_watch_debounce_ms` | `number` | `cascade` | `10000` | Debounce delay after workspace file edits before background indexing. |
| `entity_watch_debounce_ms` | `number` | `cascade` | `2000` | Debounce delay after journal/task/debt SQLite mutations before background indexing. |
| `embedding_batch_size_count` | `number` | `global_only` | `10` | Chunks vectorized per ONNX micro-batch before LanceDB checkpoint. |
| `embedding_batch_sleep_ms` | `number` | `global_only` | `20` | Cooperative sleep in ms between embedding micro-batches. |
| `throttler_min_free_mem_mbytes` | `number` | `global_only` | `2048` | Minimum free host RAM (MiB) for background indexing. |
| `throttler_critical_free_mem_mbytes` | `number` | `global_only` | `512` | Minimum free host RAM (MiB) for manual (`force = true`) indexing. |
| `throttler_max_cpu_percent` | `number` | `global_only` | `45` | Maximum global host CPU (%) before background indexing yields. |
| `onnx_intra_threads_count` | `number` | `global_only` | `2` | ONNX Runtime CPU intra-op threads for BGE-M3. |
| `max_relations_per_result_count` | `number` | `global_only` | `6` | Default 1-hop graph relations attached per `search_knowledge` result. |

## Implementation Status

| Area | Status | Notes |
| :--- | :--- | :--- |
| SQLite schemas (`agent_session_journal`, `project_todos`, `project_technical_debt`, `project_initialization_flags`, `knowledge_edges`, `journal_file_hunks`, `project_code_symbols`) | Implemented | Verified in `src-tauri/src/db.rs` (including `context_pct`, `total_tokens`, `milestone_id`, and Phase D migration). |
| MCP tools (`record_session_minutes`, `query_session_journal`, `search_conversations`, `reopen_conversation`, `manage_todo_item`, `record_technical_debt`, `raise_initialization_flag`, `resolve_initialization_flag`, `search_knowledge`, `get_file_line_insights`) | Implemented | Verified in `src-tauri/src/mcp.rs` and `src-tauri/src/capabilities/knowledge.rs` / `journal.rs`. |
| Daemon commands (`search_knowledge`, `index_project_knowledge`, `get_indexing_status`, `get_project_knowledge_graph`, `rebuild_project_knowledge_graph`, `get_file_line_insights`, `get_file_line_range_content`, `get_file_symbols`, `resolve_symbol_definition`) | Implemented | Registered in `src-tauri/src/api.rs`. |
| LanceDB `knowledge_v2`, BGE-M3 engine, throttler, AST chunker, `[plugins.semantic]` config | Implemented | Verified in `src-tauri/src/semantic/` (`store.rs`, `throttler.rs`, `chunker.rs`, `mod.rs`) and `src-tauri/src/config.rs`. |
| External audit endpoint (`POST /api/v1/audit/semantic` & `scripts/audit_semantic.sh`) | Implemented | Verified in `src-tauri/src/mcp.rs` and `scripts/audit_semantic.sh`. |
| Omnisearch (`Ctrl + K`), Devlog Journal UI, Knowledge Graph tab | Implemented | Verified presence of `OmnisearchModal.vue`, `DevlogJournalView.vue`, `ProjectKnowledgeGraphTab.vue`, `ConversationSearchModal.vue`, and `journalStore.ts`. |
| Tri-state session taxonomy (`unengaged_draft` / `trampoline` / `promoted`) | Partial | Conversation lifecycle (`agent_conversations`, `conversation_native_sessions`, `search_conversations`, `reopen_conversation`, `record_session_minutes`) is implemented; explicit `unengaged_draft` / `trampoline` state labels were not found by name in `src-tauri/src/`. |

## Related Specs

- [Observability Index](README.md)
- [Universal Telemetry, Continuous Profiling & Profiler Engines](02-universal-telemetry-and-profiling.md)
- [Configuration Schema and Cascade](../01-architecture/02-configuration-schema-and-cascade.md)
- [Git Workspace, Branches & Snapshots Cockpit](../03-projects/02-git-workspace-and-branches.md)
- [Design System Foundations](../07-ui/01-design-system-foundations.md)

Supersedes legacy: `22_session_lifecycle_and_devlog_graph.md`
