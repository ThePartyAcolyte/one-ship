# Development Log: 2026-09-11 Unified Reader Panel and Infinite Tree Explorer

**Domain:** Document Engine, Knowledge Base Navigation, Single Reader UX
**Topic:** Eradication of redundant split viewers and implementation of infinite-depth recursive document tree navigation.

---

## 1. Problem Statement & Root Cause

1. **Incomplete Tree Expansion:** The directory explorer in `ArtifactPanel.vue` was implemented using a single shallow loop (`v-for="child in cat.children"`). Clicking directories located inside `docs/` (such as `docs/specs/`) triggered `toggleFolder` but failed to render sub-items because there was no nested recursive template.
2. **Duplicate Lateral Viewers:** When selecting files, the viewer mode switched to `browse`, which split Column 3 into a 192px sidebar and a squished reader canvas ("visor lateral"). This collided with the full-width Live Artifact reader mode, creating two inconsistent and competing reading surfaces with separate state management.

---

## 2. Technical Architecture Changes

1. **Infinite Recursive Document Tree (`DocTreeNode.vue`):**
   - Created `src/components/dialogue/DocTreeNode.vue`, a self-referential recursive Vue 3 component.
   - Handles arbitrarily deep directory trees with expand/collapse chevron toggles, folder open/close state, file type icons, and modified indicators.
   - Raised Rust recursion guard from 10 to 50 levels in `src-tauri/src/kopeng/documents.rs` (`build_doc_tree`).

2. **Unified Single Reader Canvas (`ArtifactPanel.vue`):**
   - Eliminated the static lateral split view.
   - The reader canvas now occupies 100% of the panel width, offering comfortable markdown and code reading.
   - Retained the `[Reader]` vs `[Terminal Log]` header toggle.
   - Unified `displayContent` calculation in `ArtifactPanel.vue` and synchronized state selection in `dialogueStore.ts`.

3. **Three-Tier Central Navigator Menu:**
   - Accessible via the header selector button in Reader mode.
   - Quick search input filtering across all three tiers:
     - **Tier 1: Scratchpads:** Active and temporary files in `scratchpad/`.
     - **Tier 2: Project Documents:** Full recursive infinite tree under `docs/`.
     - **Tier 3: Ephemeral Artifacts:** In-memory artifacts and plans from chat turns.
   - Clicking any item across all tiers immediately opens the target in the single full-width reading panel and dismisses the navigator overlay.

---

## 3. Verification & Compliance

- `cargo check`: Finished in 1.99s with 0 errors.
- `npx vue-tsc --noEmit`: Strict TypeScript typecheck passed with 0 errors.
- `npm run build`: Production bundle built successfully in 3.05s.
- Spec 20 updated and synchronized in `docs/specs/20_live_artifacts_and_project_scratchpad.md`.
