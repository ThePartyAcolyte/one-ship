# Development Log: Viewport Responsiveness, Contextual Inspector & Undo/Redo

**Date:** 2026-09-23  
**Topic:** Mobile-First Viewport Bound Styling, Contextual Tool Filtering, In-DOM Inline WYSIWYG Text Editing, and Multi-Level Undo/Redo  
**Target:** Visual Web Builder (`src/components/workspace/ProjectVisualBuilderTab.vue`, `src-tauri/src/kopeng/visual_builder.rs`, `visual-builder-client.ts`)

---

## 1. Context & Motivation

During hands-on testing of the Visual Builder with `estudiorc`:
1. **Responsive Degradation & Horizontal Scroll:** Resizing elements while inspecting desktop view previously inserted unconditional fixed-width classes (such as `w-[914px]` or `w-[1020px]`). Because Tailwind operates on a mobile-first paradigm, these fixed pixel widths leaked directly down to mobile viewports, breaking fluid layouts and causing horizontal scrollbars.
2. **Layout Breakage on Text Elements:** Applying flexbox layouts directly to typography tags (`h1`, `p`, `span`) fractured headline words and colored spans into separate flex columns, distorting inline text flow.
3. **Missing Undo / Redo Mechanism:** Mistakes in style choices or layouts had no way to revert backwards without manually editing the source files.
4. **Contextual Tool Overload:** Tools irrelevant to the selected element (e.g. flexbox on text spans, typography controls on containers, background image swatches on headlines) caused visual clutter and operational mistakes.
5. **In-DOM Inline WYSIWYG Text Editing:** Eliminating sidebar text editing in favor of direct in-DOM editing via double-click while preserving 100% computed styling fidelity.

---

## 2. Technical Implementation Summary

### 2.1 Viewport-Bound Responsiveness (`activeBreakpointPrefix`)
- Added computed `activeBreakpointPrefix` in `ProjectVisualBuilderTab.vue`:
  - `desktop` -> `'lg:'`
  - `tablet` -> `'md:'`
  - `mobile` -> `''` (mobile-first base)
- **Surgical Width & Height Scoping:**
  - Interactive element resize events (`vb-element-resized`) now scope custom pixel dimensions strictly to the active breakpoint (e.g. `lg:w-[914px]`).
  - Automatically appends `max-w-full`, permanently preventing mobile horizontal overflow.
- **Box Model & Spacing:**
  - `applyBoxModelSide` prefixes margin and padding adjustments (`lg:mt-4`, `md:px-6`).
  - Added Quick Sizing Row (`Center`, `Auto`, `Full`, `Max 100%`, `Reset`).
  - `parseClassesForBoxModel` respects the CSS cascade, prioritizing active breakpoint tokens while falling back to tablet and base classes.
  - Automatically re-evaluates inputs on viewport switcher change.

### 2.2 Contextual Inspector Tool Filtering (`elementCategory`)
- Implemented `elementCategory` computed property classifying elements into `'text'`, `'container'`, `'interactive'`, and `'media'`:
  - **Text Elements** (`h1`–`h6`, `p`, `span`, `label`, `strong`, `em`): Exposes Typography, Text Color, Spacing, and Opacity. Hides Layout/Flexbox, Background Images, and Image Source.
  - **Container Elements** (`div`, `section`, `article`, `header`, `footer`, `nav`): Exposes Box Model, Display Modes, Appearance (solid/image bg, border, radius, shadow), and Agent Co-pilot. Hides Typography.
  - **Interactive Elements** (`button`, `a`): Exposes Typography, Appearance, Spacing, and subtle alignment.
  - **Media Elements** (`img`): Exposes Image Source & Gallery, Object Fit, and Radius. Hides Typography, Flexbox, and Background palette.

### 2.3 Mutually Exclusive Display Modes & Layout Reset
- Replaced naive flex class appending with mutually exclusive modes:
  - `Block` (`block`)
  - `Row` (`flex flex-row`)
  - `Col` (`flex flex-col`)
  - `Grid` (`grid`)
  - `Reset` (`RotateCcw` button that clears all flex, grid, alignment, and gap tokens back to default flow).
- Clicking the active mode toggles it off back to default.

### 2.4 Multi-Level Undo / Redo History Stack
- Maintained an in-memory session stack of up to 50 operations (`HistoryAction` with `classes`, `text`, `image_src`).
- Added top toolbar buttons `Undo2` and `Redo2` with dynamic tooltip descriptions and disabled states.
- Integrated keyboard shortcuts (`Ctrl+Z`, `Ctrl+Y`, `Ctrl+Shift+Z`) in both the host studio and forwarded from the canvas iframe via `visual-builder-client.ts`.

### 2.5 In-DOM Inline WYSIWYG Text Editing
- Implemented double-click inline text editing directly inside `visual-builder-client.ts` with `contentEditable = 'true'`.
- Preserves native typography, font weights, colors, and line-heights without synthetic input boxes.
- Temporarily suppresses selection overlays and badges during typing.
- Supports commit on `Enter` (single-line), `Ctrl+Enter` (multiline), or `blur`, and cancel on `Escape`.
- Enhanced backend `update_jsx_element_text` with `start_line: Option<usize>` for surgical AST targeting without colliding with duplicate strings in other components.

### 2.6 Sanitization of `Hero.tsx` in `estudiorc`
- Cleaned up broken fixed-width classes (`w-[1020px]`, `w-[914px]`, `w-[768px]`, `w-[328px]`) and duplicate `flex flex-row` attributes on the headline and span elements.

---

## 3. Verification & Compliance
- `tsc -b && vite build` in `estudiorc/frontend`: Exited 0 (clean build).
- `vue-tsc --noEmit && vite build` in `wang-kapawu`: Exited 0 (clean build).
- `cargo check` in `wang-kapawu/src-tauri`: Exited 0 (0 warnings/errors).
- Updated specification `docs/specs/27_visual_web_builder_and_onlook_stack_integration.md` (Section 11).
- Strictly respected user rules: 0 git commands executed, dedicated file editing tools only.
