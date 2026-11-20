# Devlog: 2026-09-11 — Live Artifacts, Project Scratchpad & Multi-Format Markdown Viewer

---

## 1. Context & Architectural Motivation

In prior builds of Wang-Kapawu, artifacts were treated as static, one-turn string attachments embedded within `AgentResponse` message payloads in in-memory/`localStorage` chat history. This presented multiple operational shortcomings:
1. **Redundant Content Reproduction:** Modifying an artifact forced the agent to reprint the complete document into conversational payloads, consuming quota and cluttering the chat log.
2. **Disconnected from Local Storage:** Artifacts did not exist as real, version-controllable, or tool-accessible files on disk.
3. **No Project Documentation Browsing:** Developers had no way to browse existing project specifications, guides, or scratchpad drafts from within the dialogue workspace without opening external editors.
4. **Unformatted Rendering:** Artifacts were presented as plain raw `<pre>` monospaced blocks without structured typography, tables, or list formatting.

---

## 2. Solutions Implemented

### 2.1 Specification & Roadmap Registration
- **Created Spec 20 (`docs/specs/20_live_artifacts_and_project_scratchpad.md`):**
  - Formalized two-tier project storage architecture: `docs/` (stable specifications) and `scratchpad/` (transient agent drafts and working notes).
  - Documented lifecycle policies, Git safety snapshot integration, and live auto-refresh semantics.
  - Formally registered the **Inline Document Commenting & Agent Feedback Loop** in Spec 20 Phase 3 and `TECHNICAL_DEBT.md` (#16).
  - Updated `TODO.md` with Phase 7 milestones.

### 2.2 Backend Document Engine (`src-tauri/src/kopeng/documents.rs`, `herdr.rs`, `mcp.rs`)
- **Document Management Module (`documents.rs`):**
  - Added `ensure_project_scratchpad_dir(project_path)` to auto-create `scratchpad/` on project launch or document indexing.
  - Implemented `get_project_documents(project_path)`: Scans `docs/` and `scratchpad/`, returning a structured file tree with metadata (`size_bytes`, `mtime`, `doc_type`, `is_dir`).
  - Implemented `read_project_document(project_path, rel_path)`: Reads document content with path-traversal guards ensuring files remain strictly within the project boundary.
  - Registered both commands in `src-tauri/src/lib.rs`.
- **Project-Relative Artifact Resolution (`mcp.rs`):**
  - Enhanced `execute_post_agent_response` to resolve relative file paths against the active agent's project root directory.
  - Binds the canonical relative path (`file_path`) into the `AgentResponse` payload.
- **Agent Operational Guidance (`herdr.rs`):**
  - Updated `build_agent_initial_prompt` with strict Live Artifacts and Scratchpad directives, teaching agents to write working drafts to `scratchpad/<name>.md` and permanent specs to `docs/<name>.md`, passing relative paths in `post_agent_response` and avoiding full-document chat re-dumps.

### 2.3 Multi-Format Viewer & Document Explorer (`ArtifactPanel.vue`, `dialogueStore.ts`)
- **Rich Brutalist Markdown Rendering:**
  - Integrated `marked` (zero-dependency, GFM-compliant).
  - Designed brutalist CSS styling for headers (uppercase, accent colored), lists, code fences, blockquotes, and tables.
- **Dual-Mode Header:**
  - **`[Live Artifact]`:** Follows the active artifact from the conversation stream with real-time disk change polling (every 3s), file path chip, and manual reload button.
  - **`[Docs & Scratchpad]`:** Interactive document explorer drawer displaying `docs/` and `scratchpad/` trees with directory toggling. Clicking any document renders it immediately in the reader canvas.
- **State Store Additions (`dialogueStore.ts`):**
  - Extended `DialogueArtifact` with `filePath?: string`.
  - Added `fetchProjectDocuments`, `selectProjectDocument`, and `refreshActiveArtifact` actions.

---

## 3. Technical Verification

- Backend compilation: `cargo check --manifest-path src-tauri/Cargo.toml` passed with 0 errors (0.30s).
- Frontend compilation: `npx vue-tsc --noEmit` passed with 0 errors.
- Package integrity: 0 vulnerabilities audited.
