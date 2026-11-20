# Development Log: 2026-09-12 Universal Chat Attachments & Clipboard Paste

## Overview
Implemented a comprehensive, unified attachment and contextual reference engine in Wang-Kapawu's dialogue workspace. This system supports clipboard image pasting (`Ctrl+V`), native disk file selection, internal vs. external file segregation, drag-and-drop reference placeholders (`[IMG1]`, `[FILE1]`, `[DOC1]`), and automatic prompt compilation for CLI coding agents.

## Architectural Changes

### 1. Backend IPC Commands (`src-tauri/src/kopeng/documents.rs`, `src-tauri/src/lib.rs`)
- `save_clipboard_image`: Decodes Base64 image payload from clipboard paste, writes to `.wang-kapawu/attachments/img_<timestamp>_<uuid>.<ext>`, and returns metadata (`rel_path`, `size_bytes`, `doc_type: "image"`).
- `attach_file_to_project`: Analyzes target file path. If internal (inside workspace root), calculates relative workspace path without duplication. If external, copies into `.wang-kapawu/attachments/` and returns new relative path.
- Updated `infer_doc_type` with image extensions (`png`, `jpg`, `jpeg`, `webp`, `gif`, `svg`).

### 2. Dialogue Store & UI State (`src/stores/dialogueStore.ts`)
- Generalization of `AttachedReviewFile` to support `id`, `doc_type`, `is_external`, `preview_url`, and `size_bytes`.
- Added actions: `attachClipboardImage`, `attachFileByPath`, and `openAndAttachFileDialog` (using Tauri's native dialog API).

### 3. Frontend Chat & UX Ergonomics (`src/components/dialogue/ConversationPanel.vue`, `src/components/ui/AppTextarea.vue`)
- **Clipboard paste listener (`@paste`):** Automatically detects pasted images from the OS clipboard, saves them via IPC, and renders visual thumbnail preview chips in the attachment tray.
- **File attachment button (`Paperclip`):** Invokes native system file picker to attach any workspace or external file.
- **Sequential Contextual Placeholders (`[IMG1]`, `[FILE1]`, `[DOC1]`):** Generated dynamically per draft.
- **Drag-and-Drop / Click Insertion:** Dragging any attachment chip directly into `AppTextarea` inserts its clean placeholder tag at cursor position.
- **Prompt Compilation:** Compiles clean user notes with structured `[ATTACHED CONTEXT & REFERENCES]` / `[REVIEW CONTEXT]` blocks containing accurate project-relative paths before dispatching to the agent runner.

## Verification
- Backend verified with `cargo check` (Code 0).
- Frontend verified with `vue-tsc --noEmit && vite build` (Code 0).

## Hotfix: Multichannel Clipboard Paste Support
- Added `@paste` bubbling in `AppTextarea.vue`.
- Implemented multi-tier image extraction in `ConversationPanel.vue` inspecting `event.clipboardData.files`, `event.clipboardData.items`, and asynchronous fallback via `navigator.clipboard.read()`.
- Added proactive keydown interceptor for `Ctrl+V` to ensure capture of binary clipboard bitmaps across Linux WebKitGTK / Wayland / X11 desktop environments.

## Fix: Duplicate Paste Triggers
- Removed double `@paste` listener from container `div` (delegated solely to `AppTextarea`).
- Removed redundant `Ctrl+V` keydown handler.
- Added debounce lock `isPasting` inside `processImageBlob` to ensure single-instance processing per paste event.

## Native Wayland Clipboard Integration (Omarchy / Hyprland)
- Documented Target Operating Environment & Native Platform Specification in `AGENTS.md` (Omarchy Linux, Hyprland, Wayland, `wl-clipboard`).
- Implemented `paste_wayland_clipboard_image` command in Rust backend (`src-tauri/src/kopeng/documents.rs`) executing `/usr/bin/wl-paste --type image/png` directly against the Wayland clipboard protocol.
- Wired `pasteWaylandClipboardImage` in `dialogueStore.ts` and `ConversationPanel.vue` for instantaneous, deterministic image paste without DOM synthetic event polyfills.

## Fix: Accurate Line Range Calculation in Rendered Markdown Reader
- Implemented `stripMarkdownFormatting` and `findLineRangeForSelection` in `src/components/dialogue/ArtifactPanel.vue`.
- Replaced the naive `content.indexOf(text)` fallback (which defaulted to `startLine = 1`) with a normalized line-by-line fuzzy locator.
- Ensures comments created on rendered `.md` files correctly record their physical `start_line` and `end_line` ranges in disk files.
