---
name: oneship-rea
description: Protocol and operational guidelines for AI-driven reverse engineering, binary decompilation, firmware extraction, and runtime analysis via REA (Reverse Engineer Anything) in ONE-SHIP.
version: "0.2.6"
---

# ONE-SHIP Reverse Engineering Protocol (`oneship-rea`)

This skill governs how AI agents perform binary analysis, protocol inspection, firmware unpacking, and clean-room re-implementation using the **REA (`rea-agents`)** MCP engine integrated into **ONE-SHIP**.

---

## 1. Core Principles & Evidence Mandate

1. **Zero Hallucination & Evidence-Backed Claims:**
   - Every architectural claim, function signature, protocol packet layout, or IPC route you report MUST be backed by concrete evidence extracted via REA tools (binary offsets, SHA-256 digests, symbol names, decompiled pseudocode spans, or captured network frames).
   - Never guess or fabricate internal behavior of a closed-source binary or firmware image.
2. **Project-Scoped Storage Isolation:**
   - Target binaries under investigation belong in the configured targets directory (`[plugins.rea].targets_dir` in `rula.toml` / global `config.toml`, defaulting to `<project_root>/targets/`).
   - All REA evidence bundles, decompiler caches, and temporary analysis workspaces are automatically isolated by ONE-SHIP inside `<project_root>/.one-ship/rea/`.
   - Structured visual analysis views and architecture maps for the ONE-SHIP **RE Lab** UI must be written to `<project_root>/.one-ship/rea/views/`.
3. **On-Demand Asynchronous Queue:**
   - The `rea` MCP server is routed through ONE-SHIP's `/mcp/rea` gateway (`http://localhost:8765/mcp/rea`).
   - If the project's REA engine is sleeping (`0 MB RAM`), your first MCP tool call automatically wakes it up.
   - Heavy decompilation tasks (such as Ghidra headless analysis on large ARM/x86_64 binaries) are queued asynchronously across projects so parallel workflows never collide or exhaust host memory.

---

## 2. Standard Reverse Engineering Workflow

### Phase 1: Target Triage & Format Identification
1. Inspect the configured targets directory (check `[plugins.rea].targets_dir` in `rula.toml`; defaults to `<project_root>/targets/`) to locate the target binary, firmware image, Electron `.asar` archive, Android `.apk`, or network `.har` capture.
2. Initialize a REA analysis session on the target file to verify its cryptographic digest (SHA-256), architecture (e.g., `ARM32 ELF`, `x86_64 ELF`, `PE32+`, `ASAR`, `DEX`), linked libraries, and symbol status (stripped vs. unstripped).

### Phase 2: Subsystem & Entry-Point Mapping
1. Query symbol tables, exported functions, string literals, and resource tables using REA's paged inspection tools (`inspect_analysis_view`).
2. Group discovered symbols and strings into logical subsystems (e.g., Authentication, Cloud Sync, Hardware/Framebuffer I/O, Local Persistence, IPC Routing).
3. Trace cross-references (`xrefs`) from known strings, API endpoints, or exported entry points to identify the core functions governing the target behavior.

### Phase 3: Cross-Correlation with ONE-SHIP Continuous Profiler (Dynamic + Static Synergy)
When the target binary can be executed locally or under emulation as a supervised service in `rula.toml` (`[scripts.persistent.<target_id>]`):
1. Query `list_profiler_runs` (from the `one-ship` MCP server, with `target_id` set to the service) for its runs, and `get_run_profile` for a run's CPU, memory and per-process profile; a service with a declared profiler engine keeps that engine's output file with its run (`profile_artifact_path`).
2. Pass the symbols or addresses that output identifies directly into REA's decompilation tools to inspect the exact C/C++/JS pseudocode of the active execution path.

### Phase 4: Visual Artifact Generation (`.one-ship/rea/views/`)
To make your findings accessible in ONE-SHIP's visual explorer without requiring the user to read raw assembly, persist structured JSON summaries under `<project_root>/.one-ship/rea/views/`:
- `architecture_map.json`: Subsystems, modules, and directed call-graph edges.
- `protocols_and_endpoints.json`: Discovered REST/WebSocket URLs, IPC channels, SQLite schemas, or binary file headers.
- `decompiled_highlights.json`: Curated high-value functions with their binary offset, decompiled pseudocode, and plain-language architectural explanation.

Each file has exactly one shape (a JSON object, never a bare array). ONE-SHIP rejects a file that does not match, naming the file, list, index and field; fix it and write it again. Fields marked *required* identify the item and must be non-empty strings. Every other field is optional: omit it when you did not establish it. Never write placeholders such as `"0x0"`, `"unknown"` or `"Core"`; the UI shows `—` for an omitted value.

```json
// architecture_map.json
{
  "subsystems": [
    { "id": "sync", "name": "Cloud Sync Engine" /* required */, "description": "…", "symbols": ["sync_push", "sync_pull"] }
  ],
  "call_edges": [
    { "id": "e1", "from_symbol": "sync_push" /* required */, "to_symbol": "http_post" /* required */, "offset_hex": "0x0041a8c0", "summary": "…" }
  ]
}

// decompiled_highlights.json
{
  "functions": [
    { "id": "f1", "symbol": "parse_header" /* required */, "offset_hex": "0x00402f10", "subsystem": "Cloud Sync Engine",
      "signature": "int parse_header(const uint8_t *buf, size_t len)", "pseudocode": "…", "explanation": "…" }
  ]
}

// protocols_and_endpoints.json
{
  "items": [
    { "id": "p1", "endpoint_or_key": "https://api.example.com/v2/sync" /* required */, "category": "rest",
      "protocol": "HTTPS + JSON", "offset_hex": "0x00510020", "description": "…" }
  ]
}
```

`id` is optional (ONE-SHIP numbers items without one). `category` is one of `rest`, `websocket`, `ipc`, `sqlite`, `binary_format`, `crypto`.

### Phase 5: Reporting & Clean-Room Specification
1. Deliver your response via `post_agent_response` with a concise spoken summary in `dialogue` and the technical evidence or clean-room specification in `artifact`.
2. Record all discovered architectural facts and milestones in `record_session_minutes`.
