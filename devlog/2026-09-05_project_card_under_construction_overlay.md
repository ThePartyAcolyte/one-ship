# Devlog: ProjectCard Brutalist Under Construction Blocking Overlay

**Date:** 2026-09-05  
**Topic:** ProjectCard Brutalist Under Construction Blocking Overlay  
**Status:** Completed

---

## 1. Context & Objectives
During unattended or semi-attended orchestration initialization, the project's configuration (`rula.toml`), agent personas, and services are bootstrapped by an automated builder. To protect user interaction and communicate live progress, `bosmang` delegated the creation of a visual blocking overlay:
- Reacts to initialization state from `coreStore` or active `init`/`initializer` agent in Herder.
- Renders a translucent blocking overlay (`bg-black/80 backdrop-blur-xs`) over the Project Card.
- Implements high-contrast yellow/black diagonal hazard stripes (`45deg, #000, #000 10px, #fbbf24 10px, #fbbf24 20px`).
- Displays a central monospace dynamic message bound to live agent summary events.

## 2. Changes Implemented

### A. Pinia Reactive Store (`src/stores/coreStore.ts`)
- Added `initializingProjects: Record<number, { isInitializing: boolean; message: string }>` to state.
- Enhanced `agentOutcomes` typing to include `summary?: string`.
- Added `setProjectInitializing(projectId, isInitializing, message)` action.
- Wired `handleDaemonEvent` for `agent-status-report`:
  - When `agent_id === 'initializer' || agent_id === 'init'`:
    - Updates `message` during `status === 'working'`.
    - Clears `isInitializing = false` and triggers `get_projects` rehydration upon `status === 'completed' || status === 'done'`.

### B. ProjectCard Component Overlay (`src/components/ProjectCard/ProjectCard.vue`)
- Set root container to `relative overflow-hidden`.
- Added `isInitializing` and `initStatusMessage` computed properties observing `coreStore.initializingProjects`, `coreStore.agentOutcomes`, and `herdrState.agents`.
- Implemented the blocking overlay with:
  - Top & bottom hazard tape stripes (`repeating-linear-gradient`).
  - Centered amber badge: `<Loader2 /> ORCHESTRATION UNDER CONSTRUCTION`.
  - Dynamic monospaced message container (`> {{ initStatusMessage }}`).

### C. Initialization Modal Trigger (`src/components/modals/ProjectInitModal.vue`)
- Marked project as initializing in `coreStore` immediately upon initiating unattended or semi-attended execution.

## 3. Verification & Compliance
- `npm run build` (`vue-tsc --noEmit && vite build`) passed with zero errors.
- Adheres to brutalist aesthetic, CSS variables, and boundary-safe layout.
