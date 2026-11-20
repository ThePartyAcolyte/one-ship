# Devlog: Frontend Plugin Architecture & ZoneOutlets
**Date:** 2026-09-04
**Topic:** UI Decoupling (Phase 3.0)

## Overview
Successfully implemented the foundational Vue 3 architecture for Kopeng plugins, eliminating the monolithic coupling in `ProjectCard.vue`.

## Implementation Details
1. **Mirrored Directory Structure:**
   - Established `src/kopeng/agents` and `src/kopeng/services` to mirror `src-tauri/src/kopeng/`.
   - Moved `HerderSection.vue` and `ScriptsSection.vue` out of the core components into these plugin directories.
2. **Plugin Registry & ZoneOutlet:**
   - Created `pluginStore.ts` to maintain a registry of Vue components assigned to specific `ZoneName` strings.
   - Built the generic `<ZoneOutlet zone-name="project-card-body" :context="{ project }" />` component, dynamically rendering injected components.
3. **Decoupling Emits via Window Event Bus:**
   - Stripped `$emit` chains that polluted the component tree.
   - Replaced them with a global lightweight Event Bus using `window.dispatchEvent` for actions like triggering modales (`kapawu-add-persona`, `kapawu-open-task-report`).
   - The daemon's WebSocket payloads are now globally routed via `pluginStore.dispatchDaemonEvent`.

## Technical Debt Addressed
- **Zero Dead Code:** Removed all hardcoded component references in `ProjectCard.vue`.
- **Architectural Cleanup:** The `main.ts` now bootstraps an array of `activePlugins` cleanly.

## Pending/Future Actions
- Move the core Modals (`AddPersonaModal`, `TaskReportModal`) into their respective plugin folders and inject them into a `global-modals` zone.
- Standardize the event bus into a strictly typed event manager inside `pluginStore.ts`.
