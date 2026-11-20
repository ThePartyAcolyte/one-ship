# Development Log: SQLite-Backed Whole-File Snapshots & Categorical Class Mutation Engine

**Date:** 2026-09-23  
**Topic:** Visual Web Builder Class Replacement Bugfixes, Categorical Engine, and SQLite Whole-File Snapshots

---

## 1. Problem Statement & Root Cause Analysis

1. **Destructive Class Wiping:**
   - Selecting a text alignment class (`text-center`, `text-left`) previously wiped away all other classes from the target element.
   - *Root Cause:* In `setClassToken`, when `!bp` (base/mobile breakpoint), line 992 had:
     `tokens = tokens.filter(t => !t.includes(':') && removeRegex.test(t));`
     Because `removeRegex.test(t)` was NOT negated, the filter discarded all classes except those matching the alignment regex, deleting `font-bold`, `p-4`, `bg-brand-600`, etc.
2. **Class Piling & Accumulation:**
   - Clicking between font sizes or alignments accumulated multiple classes (e.g. `text-3xl sm:text-5xl lg:text-xl` or `text-center text-left`) rather than replacing within the specific functional category.
   - *Root Cause:* Token replacement relied on naive prefix matching and incomplete regexes that did not strip competing classes across prefixes or within the same category.
3. **Color Target Overlap:**
   - `applyColor` used `new RegExp('^text-')`, which treated `text-3xl` (size) and `text-center` (alignment) as colors, stripping them when a color was applied.
4. **Line-Based AST Undo Drift:**
   - The initial line-based AST undo mechanism frequently failed when line coordinates shifted after multiline edits or when switching between elements.

---

## 2. Implemented Architecture & Solutions

### 2.1 Categorical Class Mutation Engine (Mathematically Disjoint Predicates)
- Implemented mutually disjoint utility predicates in [ProjectVisualBuilderTab.vue](file:///home/butcherwutcher/Projects/dev/wang-kapawu/src/components/workspace/ProjectVisualBuilderTab.vue):
  - `isTextAlignToken`: Matches `text-(left|center|right|justify|start|end)`.
  - `isFontSizeToken`: Matches `text-(xs|sm|base|lg|xl|[2-9]xl|\d+px)`.
  - `isTextColorToken`: Matches `text-*` while strictly excluding text alignment, font sizes, and text wrap/overflow utilities.
  - `isBgColorToken`: Matches `bg-*` while strictly excluding cover, contain, repeat, position, origin, and clip.
  - `isBorderColorToken`: Matches `border-*` while strictly excluding border width, style, and collapse.
  - `isFontWeightToken`: Matches `font-(normal|medium|bold|...)`.
  - `isBorderRadiusToken`: Matches `rounded(-...)`.
  - `isBoxShadowToken`: Matches `shadow(-...)`.
  - `isOpacityToken`, `isJustifyToken`, `isGapToken`.
- Introduced `setCategoryToken(category, newClass, isResponsive)`:
  - Replaces existing tokens only within the target category and active breakpoint.
  - Fixed `setClassToken` fallback to preserve unrelated classes: `(t.includes(':') || !removeRegex.test(t))`.

### 2.2 SQLite Whole-File Atomic Snapshots (`visual_builder_snapshots`)
- **Database Schema:**
  - Added `visual_builder_snapshots` table with index `idx_vb_snapshots_project(project_path, created_at)`.
  - Stores `id`, `project_path`, `file_path`, `action_description`, `before_content`, `after_content`, `created_at`.
- **Configurable Retention:**
  - Registered `max_visual_builder_undo_snapshots_count` (default: 50) under plugin `snapshots` in `rula.toml` and global configuration schema.
  - Automated pruning executes on insert when count exceeds the configured limit.
- **Tauri IPC Commands:**
  - `update_jsx_element_classes`, `update_jsx_element_text`, `update_jsx_image_src`: Record snapshot before disk write and return `VbMutationResult`.
  - `restore_source_file_snapshot(file_path, content)`: Reverts source files byte-for-byte to snapshot content, triggering instant Vite HMR without coordinate drift.
  - `get_visual_builder_history(project_path, limit)`: Exposes persistent history loaded on tab mount.

---

## 3. Verification & Results

- **Rust Backend:** `cargo check` and `cargo test` passed with 24/24 unit and integration tests passing.
- **Wang-Kapawu Frontend:** `npm run build` (`vue-tsc --noEmit && vite build`) passed with zero errors.
- **EstudioRC Target:** `npm run build` (`tsc -b && vite build`) passed cleanly.
- **Documentation:** Updated [Specification 27](file:///home/butcherwutcher/Projects/dev/wang-kapawu/docs/specs/27_visual_web_builder_and_onlook_stack_integration.md) (Sections 11.3 & 11.4).
