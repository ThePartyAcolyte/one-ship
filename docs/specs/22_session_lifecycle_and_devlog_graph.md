# Spec 22: Session Lifecycle, Database Devlog & Knowledge Graph Architecture
**Document Version:** 0.2.3  
**Domain:** Agent Session Lifecycle, Devlog Journal Database, Relational Project State, Knowledge Graph  
**Maintainer:** Kapawu / Omarchy Development Team  

---

## 1. Overview & Motivation

Historically, project history and conversational state were fragmented across disconnected systems:
1. **Siloed Chat Sessions:** The UI attempted to compartimentalize agent dialogue into arbitrary manual topics (`+ New Topic` dropdowns), discarding the actual underlying Antigravity CLI session threads (`agy_conversation_id`).
2. **Static Markdown Sprawl:** Development logs (`devlog/`), task backlogs (`TODO.md`), and architectural debt logs (`TECHNICAL_DEBT.md`) existed as flat markdown files on disk. While human-readable, they were invisible to relational querying, difficult for agents to inspect without expensive context dumping, and unlinked from the exact agent execution cycles that generated them.
3. **Session Waste & Startup Overhead:** Every time an agent was launched or restarted, a new Antigravity session was initialized, re-reading files and consuming tokens even if the user had not yet interacted with the agent.

Spec 22 establishes a unified, database-backed architectural foundation that resolves these limitations:
- **Continuous Conversational Stream:** Each agent maintains a single, unbroken chronological timeline in the UI. Context switches appear as inline system milestone chips rather than separate windows.
- **Tri-State Session Lifecycle:** Antigravity sessions are explicitly classified as `Idle Pool`, `Trampoline`, or `Substantive`, eliminating token waste and pruning zombie sessions.
- **First-Class Database Entities:** `devlog/`, `TODO.md`, and `TECHNICAL_DEBT.md` are elevated into structured SQLite database tables (`agent_session_journal`, `project_todos`, `project_technical_debt`), populated by agents via MCP tools.
- **The Development Knowledge Graph:** Connects specifications (`docs/specs/`), technical decisions, touched files, tasks, and technical debt directly to the Antigravity session UUID where the work occurred.
- **Transversal Semantic Search:** Decouples vector indexing into a platform-wide engine, allowing agents to find past decisions and resume relevant contexts via natural language.

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

---

## 2. Tri-State Session Lifecycle Taxonomy

Antigravity CLI sessions (`agy`) spawned by the native process supervisor progress through three distinct lifecycle states:

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

### 2.1 Idle Pool Sessions (`unengaged_draft`)
- When an agent is spawned by the supervisor, `agy` creates a session UUID.
- If the agent emits its ready signal but the user never sends a prompt, the session has `user_turns == 0`.
- **Reuse Strategy:** When the developer restarts the application or the supervisor restarts the agent, ONE-SHIP inspects the latest session for that agent. If `user_turns == 0`, it restarts `agy --conversation <uuid>` instead of spawning a new UUID. This avoids re-reading project rules and persona docs.

### 2.2 Trampoline Sessions (`trampoline`)
- If the user opens an idle session and states: *"Let's resume work on SQLite persistence"*, the agent calls `search_conversations` to locate the target Conversation and `reopen_conversation` to continue it from a Brief (Spec 30 §8).
- The session that acted as the dispatcher did not produce original technical decisions.
- Upon context handoff, this transient session is silently pruned from the turn ledger so it does not pollute history.

### 2.3 Substantive Sessions (`promoted`)
- A session is formally promoted to the persistent historical ledger when the agent invokes `record_session_minutes()`.
- This action marks the session as substantive, calculates vector embeddings, logs touched files, and enters it into the Knowledge Graph.

---

## 3. Database Devlog & Relational Project State

Spec 22 formally replaces static flat files with structured SQLite database tables inside `~/.config/one-ship/oneship.db`.

### 3.1 The Canonical Devlog Journal (`agent_session_journal`)

> [!IMPORTANT]
> **Project-First Lifecycle Invariant (Zero Agent Coupling):**
> Development logs, active tasks, and technical debt represent the chronological lifecycle of the *project as a whole*, not an isolated agent silo.
> 1. Primary hydration queries always retrieve project-wide history (`agent_id = None`), allowing inspection when no agent subprocess is open or active.
> 2. Agent filtering in the UI is strictly an optional display facet, never restricting the primary data pipeline.
> 3. Conversation reopening (`reopen_conversation`) binds dynamically to the record's originating `agent_id` rather than the caller's transient UI focus.
> 4. The Journal panel exposes the three record families as `Devlogs | Tasks | Tech Debt` sub-tabs sharing one search box and one filter-chip row; filters are display-only and never alter stored records.

Replaces the legacy `devlog/YYYY-MM-DD_<topic>.md` convention:

```sql
CREATE TABLE IF NOT EXISTS agent_session_journal (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    project_id INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    agent_id TEXT NOT NULL,
    conversation_id TEXT REFERENCES agent_conversations(id) ON DELETE SET NULL, -- ONE-SHIP Conversation (Spec 30); NULL only for rows predating Phase D
    agy_conversation_id TEXT,  -- native runner reference, when reported
    title TEXT NOT NULL,
    summary TEXT NOT NULL,
    decisions TEXT,          -- JSON Array: ["Decided X over Y", "Enforced rule Z"]
    files_touched TEXT,      -- JSON Array: ["src-tauri/src/db.rs", "src/App.vue"]
    technical_debt TEXT,     -- JSON Array: Debt items resolved or created
    tags TEXT,               -- JSON Array: ["persistence", "sqlite", "ui"]
    milestone_id INTEGER REFERENCES project_milestones(id) ON DELETE SET NULL, -- Optional strategic milestone attribution (Spec 34)
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    embedding BLOB           -- High-dimension vector for semantic discovery
);

CREATE INDEX IF NOT EXISTS idx_session_journal_proj ON agent_session_journal(project_id, agent_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_session_journal_agy ON agent_session_journal(agy_conversation_id);
CREATE INDEX IF NOT EXISTS idx_session_journal_conv ON agent_session_journal(conversation_id, id DESC);
```

> Spec 30 Phase D: Minutes are keyed by the ONE-SHIP Conversation resolved from the attested caller (Spec 12 §6); no identifier parameter is accepted. Existing databases are rebuilt once in place (rows kept, `conversation_id` NULL).

### 3.2 Actionable Tasks (`project_todos`)

Replaces static lines in `TODO.md`:

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

### 3.3 Technical Debt Ledger (`project_technical_debt`)

Replaces static entries in `TECHNICAL_DEBT.md`:

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
    milestone_id INTEGER REFERENCES project_milestones(id) ON DELETE SET NULL, -- Optional milestone attribution for health tracking (Spec 34)
    identified_at INTEGER NOT NULL,
    resolved_at INTEGER
);

CREATE INDEX IF NOT EXISTS idx_tech_debt_proj ON project_technical_debt(project_id, status, category);
```

### 3.4 Initialization Flags Ledger (`project_initialization_flags`)

Tracks decoupled gaps discovered during Phase 1 onboarding that require independent Phase 2 resolution:

```sql
CREATE TABLE IF NOT EXISTS project_initialization_flags (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    project_id INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    flag_type TEXT NOT NULL,                  -- 'docs', 'todos', 'tech_debt', 'profiler'
    title TEXT NOT NULL,
    description TEXT,
    metadata_json TEXT,
    status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending', 'resolved', 'dismissed')),
    created_at INTEGER NOT NULL,
    resolved_at INTEGER
);

CREATE INDEX IF NOT EXISTS idx_init_flags_proj ON project_initialization_flags(project_id, status);
```

---

## 4. MCP Primitives & Tool Contracts

Agents interact with the session lifecycle and knowledge graph via standardized JSON-RPC 2.0 primitives exposed on port `8765`:

### 4.1 `record_session_minutes`
Promotes the current session to the canonical devlog journal and triggers the configured `[plugins.vcs].minutes_checkpoint_mode` checkpoint (`snapshot`, `commit`, or `off`). Rejected with `isError: true` if the caller has a pending `delegate_agent_task` assignment (delegated specialists must report via `report_task_status`) or if `minutes_checkpoint_mode = "commit"` fails validation/Git hooks (in which case the journal row is rolled back and the hook output is returned). Also persists optional `commit_hash` on `agent_session_journal`.

```json
{
  "name": "record_session_minutes",
  "description": "Records structured meeting minutes and technical decisions for the active session, promoting it to the permanent devlog journal.",
  "parameters": {
    "type": "object",
    "properties": {
      "title": { "type": "string", "description": "Concise summary title of the milestone or topic" },
      "summary": { "type": "string", "description": "Executive overview of changes and progress" },
      "conversation_summary": { "type": "string", "description": "Optional. Summary of the whole current Conversation; replaces the previous one and seeds future Briefs (Spec 30 §6)" },
      "decisions": { "type": "array", "items": { "type": "string" }, "description": "Key technical trade-offs or decisions made" },
      "files_touched": { "type": "array", "items": { "type": "string" }, "description": "Relative paths to modified files" },
      "technical_debt": { "type": "array", "items": { "type": "string" }, "description": "Any technical debt incurred or resolved" },
      "tags": { "type": "array", "items": { "type": "string" } }
    },
    "required": ["title", "summary"]
  }
}
```

### 4.2 `query_session_journal`
Queries historical development journal entries and session minutes to inspect decisions, milestones, and context.

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

### 4.3 `search_conversations` and `reopen_conversation`
Replace the former `resume_agent_session` (which restarted a process with an `agy` session UUID). Both act on the caller's own project and agent, resolved from the attested identity (Spec 12 §6); neither takes identity parameters. Full contract in Spec 30 §8.

```json
{ "name": "search_conversations", "parameters": { "query": "string", "status": "active | archived (optional)" } }
{ "name": "reopen_conversation", "parameters": { "conversation_id": "string (from search_conversations)" } }
```
`reopen_conversation` closes the current Conversation, opens a new one linked by `reopened_from`, and returns the Brief of the reopened Conversation as its result.

### 4.3 `manage_todo_item`
Manipulates the relational project task backlog.

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

### 4.4 `record_technical_debt`
Logs or resolves technical debt in the relational ledger.

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

### 4.5 `raise_initialization_flag`
Emits an onboarding or bootstrap flag during Phase 1 reconnaissance.

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

### 4.6 `resolve_initialization_flag`
Resolves an initialization flag upon successful Phase 2 completion.

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

---

## 5. Transversal Semantic Search Infrastructure

Semantic search is decoupled as a **platform-wide foundation service** rather than a narrow chat utility.

### 5.1 Architecture & Core Components
1. **Scope (Prose Vectors + Deterministic AST Symbols & Git Hunks):**
   - **BGE-M3 Vector Embeddings (LanceDB `knowledge_v2`):** Indexes prose knowledge entities only — architectural specifications/documentation (`specs`: `.md`, `.txt`, `.pdf`), devlog session minutes (`journal`), tracked TODOs (`tasks`), and technical debt items (`tech_debt`). Source code and static config/HTML data files are intentionally excluded from dense vectorization to eliminate cross-domain cosine similarity noise.
   - **Deterministic AST Symbols (`project_code_symbols` in SQLite):** Multi-language Tree-sitter symbol declarations (`file_path`, `symbol_name`, `short_name`, `line_start`, `line_end`) indexed incrementally via `file_sha256` with zero ONNX inference.
   - **Causal Git Hunks (`journal_file_hunks` in SQLite):** Line-level diff ranges (`line_start`, `line_end`, `change_kind`, `source_kind`) extracted deterministically from `record_session_minutes` Git commits/snapshots (with file-level `files_touched` fallback).
2. **Embedded Vector Database (LanceDB):**
   - In-process, serverless vector database (`lancedb` Rust crate) backed by Apache Arrow columnar memory and Lance disk format.
   - Storage directory: `~/.config/one-ship/vectors.lance/` (`knowledge_v2` table, automatically dropping legacy `knowledge` on upgrade) partitioned by `project_id`.
   - Native scalar filtering combined with Vector ANN search over 1024 dimensions.
3. **Local Embedding Engine (BGE-M3):**
   - High-precision local model (`BAAI/bge-m3`) executed via ONNX Runtime (`fastembed` crate).
   - Generates 1024-dimension dense vector representations.
   - Massive context window of **8,192 tokens** enabling complete function, spec section, or session ingestion without lossy truncation.
   - Native multilingual support across English, Spanish, and programming language syntax.
4. **Adaptive Idle Throttler (System Telemetry & Resource Governance):**
   - Background indexing is strictly governed by `sysinfo` host telemetry and native process supervision:
     - `min_available_memory_bytes = 2 * 1024 * 1024 * 1024` (2.0 GiB minimum free memory gate).
     - `max_cpu_usage_percent = 45%` (global host CPU threshold gate).
     - Supervisor Task Concurrency Gate: Background indexing pauses immediately if ONE-SHIP itself is running internal project tasks (`RunTargetType::Task` in `Working` state: compilation, test suites, migrations). External OS processes outside the application boundary are never probed or assumed.
   - Execution Priority: Background embedding worker runs on E-cores / low thread count with `nice 19` OS scheduling priority.
   - Yielding: Processes micro-batches of 5–10 chunks and yields execution to Tokio (`tokio::task::yield_now().await`).
   - Interactive Indexing & Search Exemption: User-facing queries and manual user-triggered re-indexing (`force = true`) bypass idle background throttling, checking only critical memory safety ($\ge 512\text{ MiB}$ free RAM) to prevent OOM panics.

### 5.2 Multi-Language Structural & AST Chunking Pipeline
To prevent storing fragmented or arbitrary line slices, content is partitioned by semantic identity rather than arbitrary character/line counts, without biasing toward the host platform's implementation language:
1. **Codebase Chunking (Universal Recursive Scan & Two-Tier Model):**
   - **Universal Recursive Traversal:** The scanner recursively discovers code files from the project root (`root_path`), accommodating decoupled and monorepo architectures (e.g. `backend/app/**/*.py`, `frontend/src/**/*.{ts,tsx}`, `connectors/**/manifest.toml`) rather than assuming flat root-level `src/` folders. Directories already processed as architectural documentation (`docs/`, `specs/`), hidden directories (`.*`), and paths matching `ignored_patterns` are automatically skipped.
   - **Tier 1 (Declarative Tree-sitter AST Parsing):** Broad multi-language coverage using `tree-sitter` and declarative S-expression queries mapping language-specific syntax trees to canonical node categories (`function`, `method`, `class`, `struct`, `enum`, `trait`, `interface`, `impl`, `module`, `import`, `call`):
     - Supported grammars: **Rust, TypeScript/TSX, JavaScript/JSX, Python, Go, Java, C#, C, C++, Ruby, PHP, Bash, HTML, CSS, SQL, TOML, YAML, JSON**.
     - SFC wrappers (`.vue`, `.svelte`, `.astro`): extract `<script>`/`<script setup>` and `<style>` blocks and delegate them to the corresponding Tier 1 grammar.
     - Leading docstrings / doc-comments immediately preceding a declaration are attached to the symbol chunk for richer semantic context.
     - **Stable Symbol Identity Key:** `<rel_path>::<qualified_symbol>` (with deterministic `#<occurrence>` suffix when overloaded or repeated in the same file), intentionally omitting line numbers from `id` so line shifts do not create orphaned vectors in LanceDB.
   - **Tier 2 (Language-Agnostic Structural Fallback):** For languages without a Tier 1 grammar (e.g., Zig, Nix, Lua, Elixir, Haskell, Verilog), the chunker uses a **brace-depth `{}` and indentation-aware block segmenter** bounded by blank-line transitions, falling back to overlapping sliding windows (`40` lines per block, `10`-line overlap) only when no structural blocks exist.
2. **Living Specifications & Documentation (`docs/specs/*.md`, `.txt`, `.pdf`):**
   - Markdown specs (`.md`): parsed into a full heading hierarchy (`H1` through `H6`, ignoring `#` inside fenced code blocks ```` ``` ```` / `~~~`) with **dynamic bottom-up coalescing**:
     - Deep sub-headings (`H4..H6`) remain coalesced inside their parent section (`H1..H3`) when the combined block fits within `4,000` bytes, preventing over-atomization of compact sub-points.
     - When a section exceeds `4,000` bytes, it dynamically subdivides along its `H4..H6` sub-headings (coalescing consecutive small siblings $< 1,200$ bytes) and preserves the full ancestor breadcrumb (`H1 > H2 > ...`) in `title`.
     - Zero artificial byte truncation is applied to the embedding payload so BGE-M3 utilizes its full 8,192-token context window.
   - PDF documents (`.pdf`): extracted via pure-Rust PDF text extraction (`pdf-extract` / `lopdf`) and segmented into bounded page/section chunks (`file_path#page_<n>`).
3. **Database Entities (Journal, Tasks, Technical Debt):**
   - Segmented as atomic entities directly from SQLite records.
   - Entity Identity Key: `journal::<session_id>`, `todo::<item_id>`, `debt::<item_id>`.

### 5.3 Incremental Indexing, Change Detection & Two-Tier Hashing
To prevent expensive full re-indexation cycles and stale/orphaned records upon file edits:
1. **Schema Support (`content_sha256` & `file_sha256`):**
   - The LanceDB `knowledge` Arrow schema stores `file_sha256` (`Utf8`) and `content_sha256` (`Utf8`) alongside vector embeddings and line offsets.
   - Existing pre-hash tables missing these columns are cleanly recreated on schema mismatch (per Zero Backward Compatibility).
2. **Level 1 (File-Level Hash):**
   - Computes `file_sha256` from file contents. In a single initial query per `project_id`, the indexer fetches `(id, file_path, file_sha256, content_sha256, line_start, line_end)` from LanceDB. If `file_sha256` matches all stored chunks for that `file_path`, parsing and embedding are skipped completely.
3. **Level 2 (Symbol/Block-Level Diffing & Orphan Pruning):**
   - When a file has changed, it is re-parsed into chunks:
     - **Unchanged Chunk:** If `id` exists with identical `content_sha256`, the stored vector is reused **without** invoking BGE-M3; only updated `line_start`, `line_end`, and `file_sha256` metadata are persisted if line numbers shifted.
     - **Modified or Added Chunk:** Only changed or new chunks are embedded via BGE-M3.
     - **Deleted Files / Removed Symbols:** Any `id` present in LanceDB for `project_id` that is absent from the current scan is purged in bounded batch `DELETE` statements (`id IN (...)`) rather than N+1 per-row deletes.
4. **Live Runtime Status & Isolated Background Worker:**
   - An in-memory poison-safe registry (`OnceLock<RwLock<HashMap<i64, IndexingRuntimeState>>>`) tracks `is_indexing`, `last_duration_ms`, and prevents concurrent duplicate indexation runs for the same `project_id`.
   - `get_indexing_status` returns live `is_indexing` and `last_duration_ms` metrics rather than static defaults.
   - Background runs (`force == false`) execute on an isolated OS worker thread with `nice 19` priority.
5. **JIT Line Resolution:**
   - Search results resolve `file_path` against the owning project's `root_path` (looked up by `project_id`) before verifying the live line offset on disk.

### 5.4 Unified MCP Tooling, Capability Core & Daemon Commands
1. **MCP Tools (`search_knowledge`, `get_file_line_insights` via `capabilities::knowledge` / Spec 40):**
   - `search_knowledge`: Hybrid knowledge discovery combining BGE-M3 semantic search across prose (`specs`, `journal`, `tasks`, `tech_debt`) with deterministic Tree-sitter AST symbol lookup (`code` in `project_code_symbols`).
     - Parameters:
       - `query` (`string`): Natural language, technical query, or symbol name.
       - `scope` (`string`, optional, default `"all"`): Enum `["all", "code", "specs", "journal", "tasks", "tech_debt"]`.
       - `limit` (`integer`, optional, default `5`): Maximum results returned.
       - `include_relations` (`boolean`, optional, default `true`): Expands 1-hop causal (`touched`, `affects`, `documents`, `relates_to`, `governs`, `references`) neighbors from `knowledge_edges` ordered descending by edge `weight`.
       - `max_relations_per_result` (`integer`, optional): Query-time cap on the number of 1-hop `relations` returned per matched node.
       - `min_relation_score_percent` (`integer`, optional): Query-time minimum edge weight percentage (`1–100`) for returned 1-hop `relations`.
   - `get_file_line_insights`: Deterministic (non-vector) causal context lookup resolving the enclosing AST/Markdown symbol, causal `minutes` (`journal_file_hunks` + `files_touched`), anchored `debt_and_todos`, and `related_docs` specs for `(rel_path, line)`.
   - Voice Controller & Capability Core (`domain = "knowledge"`) additionally exposes `resolve_symbol_definition`, `get_file_symbols`, `get_indexing_status`, `index_project`, and `rebuild_graph`.
2. **Daemon Commands:**
   - `search_knowledge`: Dispatches hybrid search queries (LanceDB prose vectors + deterministic `project_code_symbols`) with optional graph expansion.
   - `index_project_knowledge`: Triggers incremental prose vectorization, deterministic SQLite AST symbol sync, historical `journal_file_hunks` backfill, and causal graph rebuild.
   - `get_indexing_status`: Returns live vector count, SQLite AST symbol count (`codeChunksCount`), `is_indexing`, and `last_duration_ms`.
   - `get_project_knowledge_graph`: Returns prose nodes, synthesized touched-`file` nodes, and causal/prose edges (`ProjectKnowledgeGraph`) for interactive visualization.
   - `rebuild_project_knowledge_graph`: Recomputes `knowledge_edges` in `< 300 ms` directly from stored LanceDB 1024-dim prose vectors, `journal_file_hunks`, SQLite anchors, and exact file references without re-running BGE-M3 inference.
   - `get_file_line_insights`: Deterministic (non-vector) lookup resolving the active AST/Markdown symbol at `(project_id, rel_path, line)` and returning causal `minutes` (`journal_file_hunks` with commit/snapshot diff links), anchored `debt_and_todos`, and `related_docs` (`FileLineInsights`) for the Explorer Editor Insights drawer.
   - `get_file_line_range_content`: Reads a bounded line slice (`FileLineRangeContent`) from a sandboxed project document for inline preview inside the Causal Insights drawer without switching the active editor tab.
   - `get_file_symbols`: Extracts structural Tree-sitter AST symbols or hierarchical Markdown headings with nesting `level` (`FileSymbolItem[]`) for the Explorer Editor Outline tree.
   - `resolve_symbol_definition`: Deterministic cross-project AST symbol lookup against `project_code_symbols` (`SymbolDefinitionLocation[]`) for `F12` / `Ctrl+Click` Go to Definition in the Explorer Editor.
   - Registered in `api::API_COMMANDS` and `src/types/daemonApi.ts`.

### 5.5 Decoupled Semantic Filtering & Project Catalog (`[plugins.semantic]`)
To prevent conflating repository version control exclusions (`.gitignore`) with local developer knowledge assets, semantic search exclusion is decoupled into a dedicated plugin schema in `rula.toml` (`[plugins.semantic]`):
- `enabled` (`boolean`, default: `true`): Enables or disables background indexing.
- `graph_tab_enabled` (`boolean`, default: `true`): Displays or hides the interactive `Graph` workspace tab (`ProjectKnowledgeGraphTab.vue`) without disabling semantic search or Causal Insights.
- `ignored_patterns` (`string_list`): Patterns excluded from semantic indexing (default: `target`, `node_modules`, `dist`, `build`, `.git`, `.env*`, `*.min.js`, `*.lock`, `__pycache__`, `.venv`, `venv`, `*.pyc`, `scratchpad`). Unlike `.gitignore`, local research, external docs, and API manuals are preserved.
- `included_extensions` (`string_list`): File extensions processed for deterministic AST symbol extraction and prose parsing (`rs`, `ts`, `tsx`, `js`, `jsx`, `py`, `vue`, `svelte`, `astro`, `go`, `java`, `cs`, `c`, `cpp`, `h`, `hpp`, `rb`, `php`, `css`, `sql`, `sh`, `md`, `txt`, `pdf`; `.html`/`.toml`/`.yaml`/`.yml`/`.json` are excluded from code symbol chunking).
- `extra_paths` (`string_list`): Project subdirectories to recursively index (`docs`, `specs`, `devlog`, `research`, `manuals`).
- `semantic_edge_min_score_percent` (`number`, default: `70`): Minimum cosine similarity percentage (`1–100`) required to link prose entities (`journal <-> specs`, `tech_debt/tasks <-> journal`) via `origin = "semantic"` edges.
- `max_file_size_bytes` (`number`, default: `262144`): Maximum file size in bytes (`256 KiB`, `0` disables) indexed for code and text files.
- `file_watch_debounce_ms` (`number`, default: `10000`): Debounce delay in milliseconds after workspace file edits before triggering background incremental indexing.
- `entity_watch_debounce_ms` (`number`, default: `2000`): Debounce delay in milliseconds after SQLite journal, task, or technical debt mutations before triggering background incremental indexing.
- Configurable via `ProjectConfigPanel.vue` and displayed in `ProjectOverviewTab.vue`.

### 5.6 Omnisearch UX (`Ctrl + K`), Overview Widget & First-Level `Graph` Tab (`ProjectKnowledgeGraphTab.vue`)
1. **User-Facing Instant Search (No LLM Required):**
   - The desktop app provides a global Spotlight/Omnisearch palette (`Ctrl + K`) and an `AppHeader` search button.
   - Dispatches user query directly to `daemonCall("search_knowledge", ...)`.
   - Results are rendered in <30 ms with brutalist category tags (`[CODE]`, `[SPECS]`, `[JOURNAL]`, `[TASKS]`, `[DEBT]`) and clickable 1-hop `Graph:` relation chips.
   - Clicking a result or relation chip navigates directly to the line range in the editor or opens the corresponding database record view.
2. **Project Overview Subsystems Card (`OverviewWidgetGrid.vue`):**
   - Displays total `CHUNKS`, total `GRAPH EDGES`, category breakdown (`SYMBOLS`, `SPECS`, `ENTITIES`), `AUTO-SYNC: ON/OFF`, active `MIN <n>%` threshold, last run duration, and an interactive **`Index`** button.
3. **Interactive Knowledge Graph Tab (`id: 'knowledge'`, Label: `Graph` — `ProjectKnowledgeGraphTab.vue`):**
   - First-level workspace tab powered by D3 (`d3-force`, `d3-zoom`, `d3-drag`, `d3-selection`, `d3-transition`) rendering a spatial causal knowledge graph across `Specs`, `Journal / Tasks / Tech Debt`, and `Files`:
     - **Expandable Spec Super-Nodes:** Specification documents (`specs`) start collapsed as document super-nodes (`+`) and expand in-situ (`-` or double-click) into individual Markdown sections with structural `contains` links.
     - **Directional Weighted Edges & Flow:** Quadratic Bezier curves with arrowheads (`<marker>`), automatic bidirectional arc separation (`A -> B` vs `B -> A`), animated directional flow on selection/hover, and midpoint weight pills (`touched` in accent, `documents`/`governs` in status-ok, `affects`/`relates_to` in status-warn).
     - **Physics Modes & Camera:** Pre-warmed `120`-tick initial equilibrium with `Stratified Flow` (`Specs` left -> `Minutes / Debt` middle -> `Files` right) and organic `Force` layouts, pan/zoom (`Fit`, `+`, `-`), drag-to-pin nodes, explicit-submit semantic spotlight (`Enter` or Search button), live `Min Weight` slider (`50%..100%`), `Rebuild` (<300 ms edge recomputation), and a resizable right inspector with `All / 1-Hop / 2-Hop` Ego-Graph isolation.

### 5.7 Causal Knowledge Graph (`knowledge_edges` & `journal_file_hunks`) & Reactive Auto-Indexing
To provide high-signal causal traceability without noisy AST `calls` or doc-to-code cosine matching, `knowledge_edges` combines deterministic Git/SQLite provenance with prose-only semantic links:
1. **Relational Edge & Hunk Schemas (`knowledge_edges` & `journal_file_hunks` in SQLite):**
   - `journal_file_hunks`: `(id, project_id, journal_id, file_path, line_start, line_end, change_kind, source_kind, created_at)` populated deterministically on `record_session_minutes` from `git show --unified=0` (`commit` mode), `git diff --unified=0` (`snapshot` mode), or `files_touched` fallback (`off` / historical backfill).
   - `knowledge_edges`: `(id, project_id, source_id, ..., target_id, ..., edge_type, origin, weight, updated_at_epoch_seconds)` indexed on `(project_id, source_id)` and `(project_id, target_id)`.
2. **Causal & Exact-Path Edges (`weight = 1.0`):**
   - `touched` (`origin = "git_hunk"` | `"files_touched"` | `"explicit_ref"`): `journal -> file` derived from `journal_file_hunks`.
   - `affects` (`origin = "anchor"` | `"explicit_ref"`): `tech_debt / tasks -> file` derived from optional `file_path`/`line_start`/`line_end` anchors or verified relative file paths in prose.
   - `relates_to` (`origin = "conversation"`): `tech_debt / tasks -> journal` via optional `journal_id`.
   - `governs` / `references` / `documents` (`origin = "explicit_ref"`): verified workspace relative paths cited in specs or minutes.
3. **Prose-Only Semantic Edges (`origin = "semantic"`):**
   - `documents` (`journal -> specs`, capped to top 5 per minute) and `relates_to` (`tech_debt / tasks -> journal`, capped to top 3 per item) computed via BGE-M3 cosine similarity (`>= semantic_edge_min_score_percent`).
4. **Reactive Background Auto-Indexing Protocol:**
   - Workspace file changes (`documents.rs` `notify` watcher) schedule `schedule_background_indexing_for_path` (debounced by `file_watch_debounce_ms`).
   - Database entity mutations (`record_session_minutes`, `manage_todo_item`, `resolve_todo_item`, `record_technical_debt`, `resolve_technical_debt`) schedule `schedule_background_indexing_for_entity` (debounced by `entity_watch_debounce_ms`).
   - Background runs execute only on projects already initialized (`total_records_count > 0 || code_symbols > 0`) when `[plugins.semantic].enabled == true`.
5. **External CLI Audit & Evaluation Bridge (`/api/v1/audit/semantic` & `scripts/audit_semantic.sh`):**
   - The active daemon serves `POST /api/v1/audit/semantic` on `127.0.0.1`, queried via `./scripts/audit_semantic.sh --project-id <id> [--reindex] [--candidate-min 60] [--query "..."]` without spawning a second ONNX Runtime or LanceDB instance.

### 5.8 Implementation Rollout Phases
- **Phase A (Pipeline Hardening & Multi-Language AST):**
  1. Broad Tier 1 `tree-sitter` S-expression chunker + Tier 2 brace/indentation fallback + SFC extraction (`.vue`, `.svelte`, `.astro`).
  2. Two-tier SHA-256 incremental diffing + batch orphan pruning in LanceDB.
  3. Isolated `nice 19` background worker + live `get_indexing_status` + `.pdf` extraction + JIT relative-path fix.
  4. Functional verification of local BGE-M3 retrieval quality across real queries.
- **Phase B (Hybrid Knowledge Graph):**
  1. `knowledge_edges` SQLite table + AST edge extraction (`CONTAINS`, `IMPORTS`, `CALLS`) + transversal spec/journal/debt linking.
  2. `include_relations` 1-hop expansion in `search_knowledge` (Daemon + MCP) and relation rendering in Omnisearch.

---

## 6. UI & Conversational Ergonomics

### 6.1 Continuous Chronological Feed
- The UI eliminates manual topic dropdown selectors and topic creation popovers.
- Each agent displays a clean, continuous conversational feed.
- Whenever the Conversation changes between consecutive turns, the stream renders a separator with the new Conversation's topic and a `Reopen` action that targets the Conversation above it (`reopen_conversation`, Spec 30 §8). Separators key on the ONE-SHIP `conversation_id`, not on a runner session id.

### 6.2 Lazy Hydration & Pagination
- **Default Load:** Only the latest **5 messages** are hydrated on agent selection (`limit: 5`).
- **History Trigger:** A top button `[↑ Load older messages (+10)]` paginates previous turns via `before_id` cursor without disrupting scroll position.

### 6.3 Column 3 Knowledge Modes
The right-hand reading canvas supports 4 distinct operational modes:
1. `[Live Artifact]`: Live auto-refreshing working file bound to active turn.
2. `[Docs & Scratchpad]`: Recursive filesystem tree of repository documentation.
3. `[Devlog Journal]`: Database devlog viewer rendering `agent_session_journal` records chronologically with decisions, touched files, and filters.
4. `[Terminal Log]`: Rolling live output stream from the agent's supervised process.
