# Devlog: Lightweight CodeMirror 6 IDE Capabilities, Agent Context Menu & Deterministic Graph Insights

**Date:** 2026-11-19  
**Scope:** Explorer Editor (`AppCodeEditor.vue`, `ProjectWorkspace.vue`), Knowledge Graph & Tree-sitter Backend (`src-tauri/src/semantic/`, `db.rs`, `api.rs`), Living Specs (`docs/specs/20_*`, `docs/specs/22_*`).

---

## Summary of Changes

### 1. CodeMirror 6 Language Expansion & Local Syntax Validation
- Installed `@codemirror/lint`, `@codemirror/language`, `@codemirror/legacy-modes`, and `@codemirror/lang-cpp` in `package.json`.
- Updated `src/components/ui/AppCodeEditor.vue`:
  - Expanded `getLanguageExtension` with C/C++ (`cpp()`), plus lightweight `StreamLanguage` modes for TOML, Shell/Bash, YAML, Go, Dockerfile, SQL, XML/SVG/KiCad, Ruby, INI/Properties, and Diff, falling back to plain text (`[]`) instead of forcing `markdown()`.
  - Added `@codemirror/lint` (`lintGutter()` + `linter(runSyntaxLinter, { delay: 350 })`) inspecting Lezer `syntaxTree` error nodes (`cursor.type.isError`) and strict `JSON.parse` offsets with zero external LSP overhead.
  - Added cursor tracking (`cursor-change`), syntax diagnostic counts (`diagnostics-change`), right-click context menu interception (`editor-contextmenu`), `F12` / `Ctrl+Click` Go to Definition (`goto-definition`), `Ctrl+Shift+O` (`toggle-outline`), and imperative `scrollToLine` / `focus` / `getSelectionContext` methods.
  - Styled lint ranges and diagnostic tooltips with Spec 13 semantic CSS tokens (`--ui-bg-*`, `--status-error-color`, `--status-warn-color`, `--border-color`).

### 2. Backend Deterministic Insights, Outline & Definition Lookup
- Added `GetFileLineInsightsArgs`, `FileInsightNode`, `FileLineInsights`, `GetFileSymbolsArgs`, `FileSymbolItem`, `ResolveSymbolDefinitionArgs`, and `SymbolDefinitionLocation` in `src-tauri/src/semantic/types.rs` with `#[ts(export)]`.
- Added projection-only LanceDB metadata readers (`fetch_file_chunk_metadata`, `fetch_project_code_chunk_metadata`) in `src-tauri/src/semantic/store.rs` so queries avoid deserializing 1024-dim embedding vectors.
- Added `db_get_knowledge_edges_for_chunk_ids` in `src-tauri/src/db.rs` querying `knowledge_edges` deterministically by chunk IDs.
- Implemented `get_file_line_insights`, `get_file_symbols`, and `resolve_symbol_definition` in `src-tauri/src/semantic/mod.rs` reusing the compiled `tree-sitter` AST chunker and Markdown heading parser (`chunker.rs`).
- Registered all 3 commands in `src-tauri/src/api.rs` (`API_COMMANDS` + `dispatch`) and `src/types/daemonApi.ts` (`DaemonCommands`).

### 3. Explorer IDE Drawers, Non-Disruptive Commenting & Agent Context Menu
- Added `openCommentAt` to `src/composables/useDocumentComments.ts` so review comments can be triggered cleanly at any line/selection without auto-popping on normal mouse selection in code editors.
- Updated `src/components/workspace/ProjectWorkspace.vue`:
  - Wired `AppContextMenu` to CodeMirror's `editor-contextmenu` event in both Read Mode and Edit Mode with **Ask Agent**, **Refactor with Agent** (attaching the file + pre-filling `dialogueStore.setAgentDraft` and opening `FloatingChatOverlay`), **Add Review Comment**, **Graph Insights for Line**, **Go to Definition**, and **Copy Selection**.
  - Removed `@mouseup="handleTextSelection"` from the readonly `AppCodeEditor` wrapper (preserving it on Markdown Preview) to eliminate accidental selection popups in code views.
  - Added slide-over **Outline Drawer** (`Ctrl+Shift+O`) with `AppInput` filter and click-to-line navigation.
  - Added slide-over **Graph Insights Drawer** resolving the active cursor line's indexed chunk and connected `knowledge_edges` deterministically (with direct jump-to-file/line and an **Index Project Graph** button when unindexed).
  - Added **Definition Candidates Picker** for multi-match `F12` / `Ctrl+Click` lookups, cursor coordinates (`Ln X, Col Y`), and syntax error badge (`N SYNTAX ERR`) in the file metadata footer.
  - Guarded explorer `Ctrl+A`/`Ctrl+C`/`Ctrl+X`/`Delete` shortcuts against `.cm-editor` / `isContentEditable` targets.

### 4. Living Specs Synchronization
- Updated `docs/specs/20_live_artifacts_and_project_scratchpad.md` (§8.1 and §9 Phase 5 checklist).
- Updated `docs/specs/22_session_lifecycle_and_devlog_graph.md` (§5.4 Daemon Commands).

### 5. UX Refinement: Docked Drawers, Line-Wrap Toggle, Readable Outline, Insight Inline Previews & Multi-File Tabs
- **Backend (`src-tauri/src/semantic/` + `db.rs` + `api.rs` + `daemonApi.ts`):**
  - Enriched `FileInsightNode` with `contentPreview` and `lineEnd` via LanceDB projection lookup (`store::fetch_chunk_previews_by_ids`), avoiding vector deserialization.
  - Added `signature` to `FileSymbolItem` and deduplicated redundant `(symbol_name, line_start)` entries in `get_file_symbols`.
  - Added `get_file_line_range_content` (`GetFileLineRangeContentArgs` / `FileLineRangeContent`) reading bounded file slices safely through `kopeng::documents::read_project_document`.
- **Frontend (`src/stores/projectExplorerStore.ts` + `src/components/workspace/ProjectWorkspace.vue`):**
  - Converted Outline, Graph Insights, and Comments drawers from `absolute` overlays to docked inline flex panels (`w-80 shrink-0`) next to the `flex-1 min-w-0` editor so code is never covered.
  - Added a toolbar **Line Wrap / Horizontal Scroll** toggle (`WrapText`, bound to `:line-wrapping="editorLineWrap"` in `AppCodeEditor.vue`).
  - Improved Outline rendering via `splitSymbolScope` (leaf name + parent scope subtitle + signature snippet) so long class names no longer hide method names.
  - Added `contentPreview` cards and an inline preview view (`← Back` + `Open` in tab) inside the Graph Insights drawer so users inspect linked specs/code without leaving their current file.
  - Implemented multi-file open tabs (`openDocuments`, `activateOpenTab`, `closeDocumentTab`) in the top bar with horizontal wheel scroll, symbolic `.` root breadcrumb fallback, dirty indicators, per-tab Read/Edit mode persistence, and relocated the `[Read | Edit]` switcher to the right side of the action toolbar.

### 6. Minimal Hierarchical Outline Tree (Code & Markdown)
- **Backend (`src-tauri/src/semantic/chunker.rs`, `types.rs`, `mod.rs`):**
  - Added `extract_markdown_headings` in `chunker.rs` returning raw heading titles and nesting levels (`H1..H6`) outside code fences.
  - Replaced unused `signature` on `FileSymbolItem` with `level: u8` (`1..6` for Markdown headings; `::` scope depth for code AST symbols) and removed dead `extract_symbol_signature`.
- **Frontend (`src/components/workspace/ProjectWorkspace.vue`):**
  - Replaced bordered Outline cards with a compact hierarchical tree (`outlineTree`, `visibleOutlineRows`, `activeOutlineNodeId`) showing only symbol/section names, collapsible parent chevrons, sublevel indentation, active-cursor highlighting, and subtle right-aligned line numbers (`L<n>`).

### 7. Full-Document Syntax Linting & Dedicated SFC Parsers
- Installed `@codemirror/lang-vue`, `@replit/codemirror-lang-svelte`, and `@fazelstudio/codemirror-lang-astro` in `package.json`.
- Updated `src/components/ui/AppCodeEditor.vue`:
  - Mapped `.vue`, `.svelte`, and `.astro` to dedicated SFC parsers (`vue()`, `svelte()`, `astro()`), and separated `.ts`/`.tsx`/`.js`/`.jsx` dialects cleanly (`jsx: false` for `.ts`).
  - Fixed false-positive syntax diagnostics caused by CodeMirror's initial ~3000-byte incremental parse boundary (`syntaxTree(state)`) by checking `syntaxTreeAvailable(state, doc.length)` and falling back to full-document `activeLang.parser.parse(text)` with zero artificial timeouts, plus deduplicating error offsets (`seenOffsets`).
  - Excluded `.vue`/`.svelte`/`.astro` from Lezer `isError` linting so valid SFC template directives and `<script setup lang="ts">` syntax do not produce false errors.

---

## Verification

### 1. Contract & TypeScript Bindings Export (`./scripts/run_dev.sh export-bindings`)
```text
test result: ok. 411 passed; 0 failed; 0 ignored; 0 measured; 155 filtered out; finished in 0.26s
```

### 2. Rule Verification Gate (`./scripts/verify_rules.sh`)
- `Zero TypeScript 'any' in src/`: **PASS**
- `Zero Tailwind palette classes`: **PASS**
- `Zero Result<_, String> in production Rust modules`: **PASS**
- `Zero backdrop-blur-* in components/views`: **PASS**
- `Zero hardcoded /home/butcherwutcher paths`: **PASS**
- `Zero raw bg-black/* or bg-[#...] outside ui.css`: **FAIL** solely on pre-existing lines in `RollingStatusLog.vue` and `src/components/modals/*` (0 new occurrences in `AppCodeEditor.vue`, `projectExplorerStore.ts`, or newly added sections of `ProjectWorkspace.vue`).

### 3. Silent Fallback Audit on Edited Files
Ran `grep -nE "unwrap_or|unwrap_or_default|\.ok\(\)|let _ =|catch\s*(\([^)]*\))?\s*\{\s*\}"` across all modified files:
- `src/components/ui/AppCodeEditor.vue`: 0 hits.
- `src/composables/useDocumentComments.ts`: 0 hits.
- `src/stores/projectExplorerStore.ts`: 0 hits.
- `src/components/workspace/ProjectWorkspace.vue`: 0 hits.
- `src-tauri/src/semantic/types.rs`: 0 hits.
- `src-tauri/src/semantic/chunker.rs`, `src-tauri/src/semantic/store.rs`, and `src-tauri/src/semantic/mod.rs`: 0 hits in newly added functions (all hits belong to pre-existing code).

### 4. Daemon & Build Health (`./scripts/run_dev.sh status` + `journalctl --user -u oneship-dev-daemon-build -n 25 --no-pager`)
- `oneship-dev-daemon-build`: `Finished 'dev' profile [unoptimized + debuginfo] target(s) in 31.50s` (0 errors).
- `daemon`, `daemon_watch`, `vite`, and `gui`: all `active` (`running`).
