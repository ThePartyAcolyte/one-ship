# Development Log: Agent List View Modes, Log Stream & Documentation Sync

**Date:** 2026-09-11  
**Topic:** Chat View UI Enhancements, Internal Agent Isolation, Dynamic View Modes & Live Documentation Sync  
**Status:** Completed

---

## 1. Context & Objectives
During chat workspace interactions, several UX refinements and UI alignment issues were identified:
1. When opening the Chat workspace for the first time, internal agents (e.g. `git`) were being automatically selected by default despite being hidden from the agent contacts list, presenting an unwanted initial agent state.
2. Agent contacts in `AgentList.vue` were listed flatly without honoring original project declaration order.
3. Users required flexible view modes for agent contacts: grouped by project, active agents prioritized, and dynamically grouped by AI model.
4. In `ConversationPanel.vue`, duplicate status indicator badges were rendered in both the agent title area and next to the Docs action button.
5. In `AgentList.vue`, the system log footer displayed only a single truncated log line. Users requested a multi-line, wrapped status log stream (5-6 lines) with an explicit collapse/expand toggle button.
6. Documentation files (`TODO.md`, `docs/specs/18_mcp_dialogue_channel_and_chat_overlay.md`) were lagging behind recently completed frontend and backend features.

---

## 2. Technical Implementation

### A. Initial Workspace Selection & Internal Agent Filtering
- **`src/stores/dialogueStore.ts`:**
  - Removed arbitrary auto-selection of the first available agent in `openOverlay` and `switchWorkspace`.
  - When opening the Chat workspace without a prior explicit selection, `activeAgentId` remains `null`.
  - If `!agentId`, `ConversationPanel.vue` displays a clean empty state ("Select a chat to start conversation").
  - Filtered out agents with `is_internal: true` across all chat list views.

### B. Agent Declaration Order & Metadata Enrichment
- **`src/App.vue` & `src/components/dialogue/AgentDialogueWorkspace.vue`:**
  - Refactored `allAgents` to iterate over `coreStore.projects` in order, preserving project order and declared agent sequence.
  - Attached metadata (`projectId`, `projectName`, `model`, `runner`, `dna`) to agent objects.

### C. Agent List Segmented View Modes (`AgentList.vue`)
- **`src/components/dialogue/AgentList.vue`:**
  - Added a compact segmented toggle button bar (`Proj` | `Act` | `Mod`) in both desktop sidebar and compact overlay headers.
  - **`by-project` (default):** Groups agents into collapsible sections by project name.
  - **`active-first`:** Renders a flat list prioritizing active (`working` / `idle`) agents at the top.
  - **`by-model`:** Groups agents dynamically by `agent.model || 'Unspecified'` without hardcoding any model names.

### D. Header Clean-up in `ConversationPanel.vue`
- **`src/components/dialogue/ConversationPanel.vue`:**
  - Removed the duplicate status indicator badge near the `Docs` button, preserving the primary status pill beside the agent name and portrait.

### E. Multi-Line Docked Log Stream in `AgentList.vue`
- **`src/components/dialogue/AgentList.vue`:**
  - Replaced the single-line truncated footer with a multi-line log container.
  - Rendered the last 5-6 status log entries with text wrapping (`break-words whitespace-pre-wrap font-mono text-[9px]`).
  - Added a collapse/expand toggle button (`ChevronDown` / `ChevronUp`) allowing users to collapse the footer to 1 line or expand to 6 lines.

### F. Live Documentation & Specification Synchronization
- **`TODO.md` & `docs/specs/18_mcp_dialogue_channel_and_chat_overlay.md`:**
  - Updated Spec 18 phase statuses to mark agent project grouping, draggable partition resizer, dialogue header quota badges, and initial chat state as completed.

---

## 3. Verification & Compliance
- **Build Verification:** `npm run build` (`vue-tsc --noEmit && vite build`) executed successfully with exit code 0.
- **Rules Verification:**
  - All UI elements use English labels and standard components (`AppTooltip`).
  - No application runtime processes were launched by the agent.
