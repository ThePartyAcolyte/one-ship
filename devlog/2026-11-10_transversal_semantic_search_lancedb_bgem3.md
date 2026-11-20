# Devlog: Transversal Semantic Search Infrastructure — LanceDB & BGE-M3 Integration

**Date:** 2026-11-10  
**Topic:** Embedded Vector Store (LanceDB), Local Embedding Inference (BGE-M3 INT8), Adaptive Telemetry Throttler, AST/Markdown Chunker, and `search_knowledge` Protocol Integration  
**Author:** Antigravity (Pair Programming with Developer)  

---

## 1. Context & Motivation

As projects grow in complexity, developers and AI agents need to pinpoint precise code implementations, architectural specifications, decisions, and technical debt without executing sequential, brute-force file reads. 
While external APIs (such as Gemini Embedding 2) offer large vector spaces, they introduce network latency, rate limit bottlenecks (1,000 requests/day, 100 requests/minute on free tier), and potential leakage of private codebase context.

To solve this locally with high fidelity and zero cloud friction:
1. Standardize on **LanceDB** (`lancedb = "0.41"`), an embedded, serverless vector database written natively in Rust with Apache Arrow integration (matching the technology stack proven in the user's `mnemox` project).
2. Deploy **BGE-M3** (`fastembed = "7.7"`, quantized INT8 `BAAI/bge-m3-onnx-int8`, 1024 dimensions, 8,192 tokens window) running locally via ONNX Runtime with an intra-op thread cap of 2 to preserve laptop responsiveness.
3. Integrate an **Adaptive Telemetry Throttler** using `sysinfo` to ensure background indexing only runs when host resources are healthy ($\text{RAM} \ge 2.0\text{ GiB}$, $\text{CPU} \le 45\%$, and no heavy active compilers like `cargo`, `rustc`, `vue-tsc`, or `vite`).
4. Expose the transversal search capability across the application boundary:
   - MCP Tool: `search_knowledge(query, scope?, project_id?, limit?)`
   - Daemon RPC API: `POST /api/v1/call/search_knowledge`
   - Future Spotlight Omnisearch (`Ctrl + K`).

---

## 2. Changes Implemented

### 2.1 Specification & Technical Debt Documentation
- Updated `docs/specs/22_session_lifecycle_and_devlog_graph.md` §5 (*Transversal Semantic Search Infrastructure*) defining the vector database location (`~/.config/one-ship/vectors.lance/`), schema definitions, chunking strategies, JIT line verification, and throttler gating rules.
- Logged Item 24 in `TECHNICAL_DEBT.md` (*Gemini Embedding 2 Cloud Provider & Hybrid Dual-Vector Benchmarking*), preserving future exploration of cloud-based 3072-dimension embeddings without blocking local delivery.

### 2.2 Host & System Dependencies
- Installed `protobuf` (36.1-1) via `pkexec pacman -S --noconfirm --needed protobuf` after explicit user confirmation, providing `/usr/bin/protoc` required by `lance-encoding` and `prost-build`.
- Added `lancedb = "0.41"` and `fastembed = "7.7"` to `src-tauri/Cargo.toml`.

### 2.3 Semantic Engine & Vector Store Module (`src-tauri/src/semantic/`)
1. **`errors.rs`:**
   - Defined `SemanticError` enum with strongly-typed variants (`Throttled`, `EngineInit`, `EmbeddingFailed`, `Store`, `Chunking`, `InvalidRequest`).
   - Implemented `ApiError` mapping (`status()`, `code()`) for clean integration with `oneship-daemon`.
2. **`types.rs`:**
   - Exported `KnowledgeScope` (`all`, `code`, `specs`, `tasks`, `tech_debt`, `journal`) deriving `ts_rs::TS` with `#[ts(export)]`.
   - Exported `SearchKnowledgeArgs`, `KnowledgeSearchResult`, `VectorEntityRecord`, and `IndexingStats`.
3. **`throttler.rs`:**
   - Implemented `can_run_background_indexing()` using `sysinfo::System`.
   - Checks available RAM ($> 2.0\text{ GiB}$), average CPU usage ($< 45\%$), and inspects the process table for active compiler binaries (`cargo`, `rustc`, `vue-tsc`, `vite`).
4. **`engine.rs`:**
   - Wrapped `fastembed::Bgem3Embedding` in a thread-safe `OnceLock<Arc<Mutex<Option<Bgem3Embedding>>>>`.
   - Configured `Bgem3InitOptions::new(Bgem3Model::BGEM3Q)` with cache directory `~/.cache/one-ship/models/` and 2 intra-op threads.
   - Exposed `embed_single(text)` and `embed_batch(texts)`.
5. **`store.rs`:**
   - Implemented embedded LanceDB connection at `~/.config/one-ship/vectors.lance/`.
   - Defined 1024-dimension Apache Arrow fixed-size list schema.
   - Implemented `upsert_records`, `delete_entity`, and `search_knowledge` with Cosine distance ANN query and scalar predicate pushdown via `only_if`.
   - Mapped Cosine distance in $[0, 2]$ to normalized similarity score $[0.0, 1.0]$.
6. **`chunker.rs`:**
   - Multi-language code chunker with symbol regex extraction for functions, classes, and structs across Rust, TypeScript/JavaScript, Python, C/C++, and sliding window fallback.
   - Markdown specification chunker with header hierarchy splitting and anchor slug generation.
   - SHA-256 two-tier content hashing (`hash_text`).
7. **`mod.rs` & `tests.rs`:**
   - Query handler `search_knowledge` with JIT file inspection for dynamic line drift compensation.
   - Project indexer `index_project_knowledge` scanning `docs/specs/*.md`, source directories (`src/`, `core/`, `api/`), SQLite tasks (`project_todos`), and technical debt (`project_technical_debt`).
   - Unit test suite covering hashing, chunking, telemetry throttler safety, and Cosine math invariants.

### 2.4 Daemon API, MCP & TypeScript Protocol Alignment
- Registered `search_knowledge` in `api::API_COMMANDS` and `api::dispatch` (`src-tauri/src/api.rs`).
- Exposed `search_knowledge` tool schema and handler in `src-tauri/src/mcp.rs`.
- Updated `src/types/daemonApi.ts` mirroring the command signature with ts-rs generated types (`KnowledgeScope.ts`, `KnowledgeSearchResult.ts`, `SearchKnowledgeArgs.ts`).

---

## 3. Verification & Results

- **Backend Rust Compilation:**
  - `cargo check --manifest-path src-tauri/Cargo.toml` passed with **0 errors and 0 warnings**.
  - `cargo test --manifest-path src-tauri/Cargo.toml --lib api::tests::api_commands_match_the_frontend_map` passed cleanly (`ok. 1 passed; 0 failed`).
  - `cargo test --manifest-path src-tauri/Cargo.toml semantic` passed 13/13 tests (`test result: ok. 13 passed; 0 failed`).
- **Frontend Typecheck & Build:**
  - `npm run build` (`vue-tsc --noEmit && vite build`) passed cleanly (`✓ built in 6.20s`).
