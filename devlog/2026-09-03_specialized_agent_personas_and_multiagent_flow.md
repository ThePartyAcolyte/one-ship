# Work Session Log: Specialized Multi-Agent Personas & Operational Playbooks

**Date:** 2026-09-03  
**Status:** Completed  
**Branch:** master  

---

## 1. Objectives & Context
Establish a specialized multi-agent division of labor for the Wang-Kapawu orchestrator, defining domain-specific personas with prioritized documentation contexts and dedicated operational playbooks in `.agents/personas/`:
1. **`bosmang`:** Lead Systems Architect & Supervisor (governance, backend, SQLite, skills updating).
2. **`frontend`:** UI/UX & Brutalist Component Specialist (Vue 3, Tailwind, standard `App*` components, narrow viewports).
3. **`mcp`:** Model Context Protocol & Tools Integration Specialist (Axum Streamable HTTP daemon on port 39281, JSON-RPC 2.0 schemas, platform workflows).
4. **`lang_belta`:** Belter Creole Linguistic & Localization Specialist (3-tier i18n, lexicon, Expanse Belter Creole dialect).

---

## 2. Implementations

### A. Persona Playbooks (`.agents/personas/`)
Created detailed, version-controlled markdown specifications for each specialist:
- [`.agents/personas/bosmang.md`](file:///home/butcherwutcher/Projects/dev/wang-kapawu/.agents/personas/bosmang.md): Identity, Outside-In architecture, SQLite persistence, skills versioning, and operational governance.
- [`.agents/personas/frontend.md`](file:///home/butcherwutcher/Projects/dev/wang-kapawu/.agents/personas/frontend.md): Standardized components, brutalist styling, smart edge boundary detection, anti-mocking invariants.
- [`.agents/personas/mcp.md`](file:///home/butcherwutcher/Projects/dev/wang-kapawu/.agents/personas/mcp.md): Streamable HTTP stateless architecture, JSON-RPC 2.0 error handling, tool schemas, and local security.
- [`.agents/personas/lang_belta.md`](file:///home/butcherwutcher/Projects/dev/wang-kapawu/.agents/personas/lang_belta.md): 3-tier translation strategy, Nick Farmer Belter grammar, and lexicon parity.

### B. Declarative Registration in `rula.toml`
Updated the root [`rula.toml`](file:///home/butcherwutcher/Projects/dev/wang-kapawu/rula.toml) to register all 4 personas under `[agents.<name>]`, wiring their `system_prompt` and prioritizing their `context_docs` to inject their respective playbooks and domain specs when launched in Herder.

---

## 3. Verification
- `rula.toml` reloaded automatically via the background `spawn_project_watcher` daemon with zero user intervention.
- The 4 agents are registered, visible in the UI under the `PROJECT` row, and ready for autonomous multi-agent delegation.
