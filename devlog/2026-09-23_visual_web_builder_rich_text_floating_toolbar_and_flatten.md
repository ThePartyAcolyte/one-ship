# Development Log: Visual Web Builder Rich Text Floating Toolbar & Block Flattening

**Date:** 2026-09-23  
**Topic:** Rich Text Floating Toolbar, Smart Span Merging, Block Flattening to Root Style, and Inner JSX AST Replacement  
**Scope:** `wang-kapawu` (Backend, Studio Bridge, Templates), `estudiorc` (Target Testbed Frontend)

---

## 1. Context & Objectives

Following user feedback on text editing limitations within the Visual Web Builder, the following core gaps were identified:
1. **Full-Node Text Destruction:** Editing text previously replaced the whole element text with plain `innerText`, destroying all internal formatting tags (`<span>`, `<strong>`, `<em>`, `<a>`).
2. **Missing Partial Text Formatting:** Users could not visually highlight specific words (e.g. within an `<h1>` or `<p>`) and apply bold, italic, underline, or theme colors.
3. **Risk of Infinite Tag Nesting ("Tag Soup"):** Iteratively modifying styles on text fragments could generate deeply nested spans (`<span><span>...</span></span>`).
4. **Need for Instant Block Reset:** Users required a direct mechanism to select an entire text container and flatten it cleanly to plain text adopting the container's root styling.

---

## 2. Implemented Architecture & Changes

### A. Rich Text Floating Toolbar (`visual-builder-client.ts`)
- **Interactive Floating Toolbar (`__vb_rich_text_toolbar__`):**
  - Listens to `selectionchange` during `isInlineEditing`.
  - When a non-empty text range is selected within `inlineEditingEl`, calculates `range.getBoundingClientRect()` and dynamically positions a brutalist floating toolbar (`#18181b`, border `#3f3f46`, `z-index: 1000002`) above the selected text.
  - Automatically hides when the selection collapses or on blur/commit.
- **Formatting Actions:**
  - **B (`font-bold`):** Toggles `font-bold` class on the selected range.
  - **I (`italic`):** Toggles `italic` class on the selected range.
  - **U (`underline`):** Toggles `underline` class on the selected range.
  - **Color Palette Popover:** Shows clickable swatches containing active project theme colors (`text-primary`, `text-accent`, etc.) plus base palette, applying `text-[token]`.
  - **Clear Selection Formatting (`Limpiar`):** Strips formatting tags from the highlighted range.
- **Smart Span Merging & Normalization:**
  - If a span already wraps the selection, modifies its classes in place rather than nesting child spans.
  - Automatically un-wraps spans whose class list becomes empty.

### B. Block Flattening to Root Style (`visual-builder-client.ts` & `ProjectVisualBuilderTab.vue`)
- **`flattenBlockToRootStyle(el)`:**
  - Wipes out all child tags within the container (`el.textContent = el.innerText`).
  - Immediately restores pure root container styling without any nested tags.
  - Dispatches `vb-element-text-updated` with `actionDesc: 'Reset to root style (Aplanar texto)'` for instant AST disk persistence and undo snapshot.
- **Host UI Integration:**
  - Added a "Reset to Root Style (Aplanar texto)" action button directly in the Section C Typography sidebar in `ProjectVisualBuilderTab.vue`.
  - Added "Aplanar Bloque" button in the canvas floating toolbar.
  - Added `vb-flatten-selected-block` IPC bridge handler in `useOnlookClientBridge.ts`.

### C. Clean JSX Normalization & AST Inner Body Engine (`src-tauri/src/kopeng/visual_builder.rs`)
- **JSX Serializer:** Converts DOM `class="..."` to JSX `className="..."` upon commit.
- **Surgical Inner Body AST Mutation:**
  - Implemented `find_closing_tag_bounds` in Rust to locate the matching closing tag `</tag>` for any JSX element coordinate.
  - Replaces the exact inner body between `tag_end` and `close_start` with the updated JSX markup.
  - Fully supports both plain text and rich JSX children without corrupting element attributes or sibling nodes.
  - Automatically creates a SQLite snapshot for complete `Ctrl+Z` / `Ctrl+Y` undo/redo fidelity.

### D. Single Unified Reset Action (Anti-Redundancy)
- Replaced the redundant pair of "Limpiar" and "Aplanar Bloque" buttons with a single unified action: **`Aplanar / Limpiar`** (`Restablecer a texto plano limpio adoptando el estilo raíz`).
- Completely strips any formatting tags and resets the text cleanly to the root container's style, avoiding user confusion.

### E. JSX String Style Crash Fix & Attribute Sanitizer
- Diagnosed that setting `span.style.color = hex` in the live DOM was serialized as `style="color: rgb(...);"` which crashed React/TypeScript (`TS2559: Type 'string' has no properties in common with type 'Properties'`).
- Sanitized `Hero.tsx:25` back to valid JSX (`<span className="text-brand-700">explicado en simple.</span>`).
- Hardened `serializeToJsx` to strip any `style="..."` attribute strings upon serialization, guaranteeing that only valid Tailwind classes (`className="text-..."`) reach React.

### F. Canonical Platform Template
- Synchronized `/home/butcherwutcher/Projects/dev/wang-kapawu/templates/visual-builder-client.ts` with the enhanced client bridge so all newly scaffolded projects immediately inherit rich text capabilities.

---

## 3. Verification & Governance Compliance

- **TypeScript / Vue Verification:** `npm run build` (`vue-tsc --noEmit && vite build`) passed with 0 errors.
- **Target Frontend Verification:** `npx tsc --noEmit` in `estudiorc/frontend` passed with 0 errors.
- **Rust Backend Verification:** `cargo check` in `src-tauri` passed with 0 errors.
- **Zero Git Command Ban:** 0 git commands executed.
- **Strict File Tool Suite:** All edits performed via dedicated file tools.
