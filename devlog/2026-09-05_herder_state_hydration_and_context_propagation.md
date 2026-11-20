# Devlog: Herder State Hydration & Context Propagation Fix

**Date:** 2026-09-05  
**Topic:** Herder State Hydration & Context Propagation Fix  
**Status:** Completed

---

## 1. Issue Description
Upon app launch or Vite HMR refresh, agents and Herder workspace connection intermittently disappeared from the Project Card in the Wang-Kapawu GUI, rendering `// UNLINKED` and `No personas`. Backend investigations confirmed all Herder workspaces and agent sessions were fully active.

## 2. Root Cause Analysis
1. **App Hydration Race Condition**: `App.vue::onMounted()` fetched projects (`invoke('get_projects')`) but never invoked `updateAllHerdrStates()`, relying 100% on asynchronous daemon WebSocket events (`herdr-states-updated`). Under socket reconnects or backoff delays, `coreStore.herdrStates[project.id]` remained `undefined`.
2. **Context Disconnect in ProjectCard**: `<ZoneOutlet zone-name="project-card-body">` in `ProjectCard.vue` received `:context="{ project }"`, failing to forward `herdrState`, `agentOutcomes`, and `runningServices` to dynamic plugin components.
3. **Reactive Fallback Gap**: `HerderSection.vue` strictly queried `coreStore.herdrStates[project.value?.id]` without falling back to injected props context.

## 3. Changes Implemented
- **`src/App.vue`**:
  - Injected `await updateAllHerdrStates()` immediately after `get_projects` in `onMounted()`, performing synchronous IPC hydration at launch.
  - Linked `kapawu-refresh-project` event to `refreshProjects()`.
- **`src/components/ProjectCard/ProjectCard.vue`**:
  - Forwarded `{ project, herdrState, agentOutcomes, runningServices }` into `<ZoneOutlet :context="...">`.
- **`src/kopeng/agents/components/HerderSection.vue`**:
  - Added fallback in `herdrState` and `agentOutcomes` computed properties to read from `props.context`.
- **`src/kopeng/services/components/ScriptsSection.vue`**:
  - Added fallback in `isServiceRunning` to check `props.context?.runningServices`.

## 4. Verification
- `npm run build` (`vue-tsc --noEmit && vite build`) passed with zero errors.
- Verified backend Herder status via `herdr workspace list` and MCP `list_projects`.
