# Devlog: Dual-Gutter Diff Line Numbers & Centralized Reader Navigation

**Date:** 2026-09-20  
**Topic:** Diff Viewer Line Numbers, Dual Gutters & Direct File Reader Integration

---

## 1. Context & Motivation
Following the integration of the unified `Git & Snapshots` cockpit, inspecting diffs required further technical refinement:
1. Diff output was rendered as raw text without line numbers, making it difficult to pinpoint modified lines during reviews.
2. Reviewing a diff often prompted the developer to edit or read the full file, but there was no immediate path from the diff stage into the workspace Reader/Editor.
3. Wang-Kapawu possesses a centralized navigation system (`dialogueStore.navigateTo`) designed for cross-tab and cross-project routing, which needed to be utilized rather than ad-hoc state mutations.

---

## 2. Key Implementations

### A. Reactive Diff Parsing Engine (`ProjectGitSnapshotsTab.vue`)
- Defined `ParsedDiffLine` interface:
  ```ts
  interface ParsedDiffLine {
    type: 'header' | 'hunk' | 'added' | 'removed' | 'context';
    oldLineNo?: number;
    newLineNo?: number;
    content: string;
  }
  ```
- Implemented `parsedDiffLines` computed parser:
  - Detects hunk headers via regex `/^@@\s+-(\d+)(?:,\d+)?\s+\+(\d+)(?:,\d+)?\s+@@/`.
  - Maintains dual running line counters (`currentOldLine` and `currentNewLine`).
  - Correctly increments counters for added (`+`), removed (`-`), and context lines.
  - Strips leading diff indicator characters in the content column while preserving them in a dedicated indicator gutter for clean copy-pasting.

### B. Dual-Gutter Diff Stage Layout
- Gutter 1: Old line number (`w-9 text-right pr-2 select-none text-[8px] opacity-40 bg-black/25 border-r border-[var(--border-color)]/30 font-mono`).
- Gutter 2: New line number (`w-9 text-right pr-2 select-none text-[8px] opacity-40 bg-black/25 border-r border-[var(--border-color)]/30 font-mono`).
- Gutter 3: Indicator sign (`w-4 text-center select-none font-bold text-[9px]`: `+`, `-`, `@`, ` `).
- Code Column: `flex-1 px-1 whitespace-pre break-all` with brutalist syntax colors:
  - Added lines: `bg-emerald-950/25 text-emerald-400`
  - Removed lines: `bg-rose-950/25 text-rose-400`
  - Hunk headers: `bg-sky-950/30 text-sky-400 border-y border-sky-500/20`
  - Headers: `text-[var(--dim-color)] bg-black/40 font-bold`

### C. Centralized Reader Navigation (`dialogueStore.navigateTo`)
- Implemented `openFileInReader(filePath: string)`:
  ```ts
  async function openFileInReader(filePath: string) {
    if (!props.project || !filePath) return;
    await dialogueStore.navigateTo({
      projectId: props.project.id,
      projectPath: props.project.path,
      tab: 'files',
      filePath,
    });
  }
  ```
- Trigger locations:
  - **Touched Files List (left sub-panel):** Quick `<Eye />` icon button on each file row.
  - **Diff Stage Header:** Prominent `[ OPEN IN READER ]` action button when a specific file is selected.

### D. Zero Dead Code Cleanup in `FloatingChatOverlay.vue`
- Purged orphaned, unreferenced metrics and account reload functions (`isAccountMismatched`, `reloadAgentWithNewAccount`, `sessionMetrics`, `formatTokenCount`, `contextBadgeClass`, `contextBarClass`, `contextTooltip`, `tokenBreakdownTooltip`) left behind by earlier gauge component refactors.

---

## 3. Verification & Validation
- **TypeScript:** `npx vue-tsc --noEmit` passed with 0 errors.
- **Vite Production Build:** `npm run build` completed cleanly in 3.67s.
- **Specifications Updated:** `docs/specs/09_vcs_and_logging_architecture.md` updated with Section 5.
