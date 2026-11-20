# Devlog: Inline Document Commenting, Codebase Tree & Agent Review Feedback Loop (Spec 20 Phase 3)

**Date:** 2026-09-11 / 2026-09-12  
**Build:** 0.2.1  
**Status:** Completed & Validated

---

## 1. Overview & Context
Spec 20 Phase 3 establishes an interactive code and document reviewing feedback loop in Wang-Kapawu. Rather than having developers manually describe file paths, line numbers, and snippets in the chat box, the document reader (Column 3) now allows inline text selection, floating comment creation with voice dictation (OpenDictate D-Bus integration), SQLite comment persistence, structured review prompt packaging, and autonomous agent resolution via MCP tools (`resolve_document_comment` and `list_document_comments`).

---

## 2. Key Architecture & Changes

### A. SQLite Persistence & Backend IPC (`src-tauri/src/db.rs`, `src-tauri/src/kopeng/documents.rs`, `src-tauri/src/lib.rs`)
- Created `document_comments` SQLite table with columns:
  `id`, `project_id`, `file_path`, `start_line`, `end_line`, `selected_text`, `comment_text`, `author`, `status`, `created_at`, `resolved_at`, `resolved_by`.
- Implemented CRUD functions:
  `db_insert_document_comment`, `db_list_document_comments`, `db_resolve_document_comment`, `db_delete_document_comment`.
- Extended `get_project_documents` with scope parameter (`"docs"` vs `"codebase"`), filtering build/dependency artifacts (`target`, `node_modules`, `.git`, `dist`, `.gemini`, `brain`).
- Registered IPC commands:
  - `create_document_comment_cmd`
  - `list_document_comments_cmd`
  - `resolve_document_comment_cmd`
  - `delete_document_comment_cmd`

### B. MCP Tools for Agent Comment Resolution (`src-tauri/src/mcp.rs`)
- Implemented `list_document_comments` tool: allows agents to query pending or resolved comments for any file.
- Implemented `resolve_document_comment` tool: marks comments as resolved, records timestamp and author, and emits `DocumentCommentResolved` SSE event.

### C. Dialogue Pinia Store (`src/stores/dialogueStore.ts`)
- Added `DocumentComment` and `AttachedReviewFile` interfaces and state (`documentComments`, `attachedReviewFiles`, `explorerScope`).
- Added getters `activeDocumentComments` and `activeDocumentOpenCommentCount`.
- Added actions `fetchProjectDocuments`, `selectProjectDocument`, `createDocumentComment`, `resolveDocumentComment`, `deleteDocumentComment`, `attachFileToChat`, `detachFileFromChat`, and `clearAttachedReviewFiles`.
- Added real-time SSE listener for `DocumentCommentResolved` events.

### D. Multi-Format Reader Canvas (`src/components/dialogue/ArtifactPanel.vue`)
- **Scope Switcher:** Added segmented toggle (`Docs` vs `Codebase`) in the file explorer header.
- **Header Actions:** Added `[+ Attach to Chat]` / `[✓ Attached]` button with live open-comment count badge and `[Comments (N)]` drawer toggle.
- **Text Selection & Floating Popover:** Detects text selection, calculates exact line ranges (`L<start>-L<end>`), and presents a popover with comment textarea, OpenDictate voice mic (`reserve_voice_capture`), and `[Save]` / `[Cancel]` buttons.
- **Inline Comments Drawer:** Collapsible slide-over drawer displaying line badges, quoted snippets, comment text, timestamps, and `[Resolve]` / `[Delete]` actions.

### E. Chat Review Packaging (`src/components/dialogue/ConversationPanel.vue`)
- Added attached review files tray above the message input area showing file names, comment count pills, and detach `[×]` buttons.
- Updated `send()` method: automates construction of structured review prompts:
  ```markdown
  [REVIEW CONTEXT: N FILE(S) ATTACHED]

  ### File: <path>
  - [Comment #<id>] Lines L<start>-L<end>: "<selected_snippet>"
    Feedback: "<comment_text>"

  ---
  User Note: <user_input_text>
  ```
- Auto-clears attached review files after sending.

---

## 3. Verification & Compliance
- `npm run build`: Passed with 0 errors (`vue-tsc` clean).
- `cargo check`: Passed with 0 errors.
- Updated `TODO.md` and `TECHNICAL_DEBT.md` (Item #16 marked resolved).
