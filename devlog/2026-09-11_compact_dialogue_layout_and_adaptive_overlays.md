# 2026-09-11: Compact Dialogue Layout, Unified Header Plane & Zero-Reflow Overlays

## 1. Context & Motivation
In compact display environments (laptops, split-screen terminal setups, or narrow viewports), displaying three simultaneous columns (Agent List at 192px + Conversation Panel + Artifact Reader at 380px–540px) caused extreme horizontal compression:
1. The message bubbles, prompt input, and controls were severely squished and wrapped unreadably.
2. The headers of the three columns had disparate heights, creating visual noise and vertical discontinuity.
3. Users had no comfortable way to read wide Markdown specifications, tables, or terminal logs without shrinking the chat to unusable dimensions.

---

## 2. Changes Delivered

### 2.1 Unified Header Height & Plane Continuity (`h-10` / 40px)
- Aligned `AgentList.vue`, `ConversationPanel.vue`, and `ArtifactPanel.vue` to a strict height: `h-10 min-h-[40px] max-h-[40px] px-3 bg-black/30 border-b border-[var(--border-color)] flex items-center justify-between`.
- Ensures all titles, badges, topic pickers, mode pills, and action controls align on a single uninterrupted horizontal plane across the entire workspace.

### 2.2 Collapsible Agent Sidebar with Zero-Reflow Flyout (`src/components/dialogue/AgentList.vue`)
- **Compact Rail (48px):** When `isCompact` is active, collapses into a clean 48px rail (`w-12`) displaying agent initials/avatar icon, status dots (`idle`, `working`, `unlaunched`), unread notification badges, and a minimal system log history button.
- **Hover Flyout Overlay (`z-40`):** Hovering over the rail expands an absolute flyout container (`absolute inset-y-0 left-0 z-40 w-60 bg-[var(--bg-color)] shadow-2xl border-r border-[var(--border-color)]`) over the left portion of the chat without shifting or reflowing the conversation stream.

### 2.3 Adaptive Full-Chat Document Reader Overlay (`src/components/dialogue/ArtifactPanel.vue`, `src/stores/dialogueStore.ts`)
- **State Integration (`dialogueStore.ts`):** Added `isArtifactPanelOpen` flag and `toggleArtifactPanel()` action. Clicking artifact chips in chat or selecting documents automatically expands the panel.
- **Header Toggle Button (`ConversationPanel.vue`):** Added `[📑 Docs]` toggle button in the chat header with an artifact count badge and active state styling.
- **Full-Chat Overlay in Compact View:**
  - When expanded in compact view, `ArtifactPanel` overlays the conversation space (`absolute inset-y-0 right-0 left-12 z-30 bg-[var(--bg-color)] shadow-2xl border-l border-[var(--border-color)]`), providing 100% width for Markdown tables, code blocks, diffs, and live terminal buffers.
  - Header features a `[→ Collapse]` button and listens to `Escape` key to instantly return to the conversation stream.
  - In wide/desktop mode, smoothly displays side-by-side or collapsible on demand.

### 2.4 Relative Container Stacking (`src/components/dialogue/AgentDialogueWorkspace.vue`)
- Set `relative overflow-hidden` on the 3-column container to anchor flyout and reader overlays precisely within the workspace bounds.

---

## 3. Verification
- **Frontend TypeScript:** `npx vue-tsc --noEmit` passed with 0 errors.
- **Rust Backend:** `cargo check --manifest-path src-tauri/Cargo.toml` passed with 0 errors and 0 warnings.
