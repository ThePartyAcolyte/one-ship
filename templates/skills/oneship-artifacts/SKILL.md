---
name: oneship-artifacts
description: Triple-mode artifact protocol: ephemeral in-memory payloads, working scratchpads, and permanent docs.
version: 0.2.5
---

# ONE-SHIP Triple-Mode Artifact Protocol

This skill establishes the rules for delivering technical content, managing evolving scratchpad files, and rendering ephemeral reports in the ONE-SHIP dialogue overlay.

---

## 1. The Three Artifact Types

| Artifact Type | Storage Location | Nature & Purpose | When to Use |
| :--- | :--- | :--- | :--- |
| **Ephemeral Artifact** | **In-Memory / SQLite** (No disk file) | **Point-in-Time & Immutable:** Attached directly to the chat turn bubble. Renders with an `[EPHEMERAL]` badge. | One-off tabular reports, database query results, diagnostic outputs, or static diffs that will not evolve. |
| **Working Scratchpad** | `scratchpad/<name>.md` | **Iterative Working Memory:** Bound to the disk watcher. Refreshes live in the UI when edited. | Evolving task plans, step-by-step refactoring roadmaps, multi-turn analysis, or draft documents. |
| **Permanent Documentation** | `docs/<name>.md` | **Stable Knowledge Base:** Version-controlled in Git. | Formal architecture specifications, design contracts, ADRs, and developer runbooks. |

---

## 2. Decision Matrix: How to Deliver Artifacts

```text
                     ¿Qué tipo de contenido técnico necesitas entregar?
                                             │
             ┌───────────────────────────────┼───────────────────────────────┐
             ▼                               ▼                               ▼
 [ Reporte puntual o estático ]    [ Plan de trabajo evolutivo ]   [ Especificación formal ]
 (ej. tabla de BD, diff puntual)   (ej. roadmap de refactor)       (ej. arquitectura, API)
             │                               │                               │
             ▼                               ▼                               ▼
 ┌──────────────────────┐        ┌──────────────────────┐        ┌──────────────────────┐
 │ ARTEFACTO EFÍMERO    │        │ SCRATCHPAD EN DISCO  │        │ DOCUMENTACIÓN FORMAL │
 │ • Envía el payload   │        │ • Escribe archivo en │        │ • Escribe archivo en │
 │   inline en          │        │   scratchpad/*.md    │        │   docs/*.md          │
 │   post_agent_response│        │ • Pasa la ruta en    │        │ • Pasa la ruta en    │
 │ • NO crees archivo   │        │   post_agent_response│        │   post_agent_response│
 │   en disco           │        │ • Edita en disco     │        │ • Versionado en Git  │
 └──────────────────────┘        └──────────────────────┘        └──────────────────────┘
```

---

## 3. Operational Rules

### A. Delivering Ephemeral Artifacts (In-Memory)
- Pass the raw technical content (markdown table, code snippet, diff) directly into the `artifact` argument of `post_agent_response`.
- Do NOT create a file on disk for one-off information that does not require ongoing editing.

### B. Delivering Live Disk Artifacts (`scratchpad/` and `docs/`)
- **CRITICAL WORKSPACE BOUNDARY (Project Root Only)**:
  * Working scratchpads and documentation files MUST ALWAYS be written directly inside the active project workspace root (`<project_root>/scratchpad/<name>.md` or `<project_root>/docs/<name>.md`).
  * STRICTLY FORBIDDEN: NEVER write scratchpads or project artifacts into the agent runner's internal brain, app data, cache, or temporary directories (e.g. `~/.gemini/antigravity-cli/brain/`, `/tmp/`, etc.).
  * Always use the project root when constructing paths for `write_to_file` and `replace_file_content` (e.g. `/path/to/project/scratchpad/copy-revision.md`).
- Write or modify the file on disk using standard file edit tools (`write_to_file`, `replace_file_content`).
- In `post_agent_response`, pass the relative path in the `artifact` argument (e.g. `artifact="scratchpad/plan.md"` or `artifact="docs/specs/auth.md"`).
- The ONE-SHIP Artifact Panel automatically attaches to the file and updates live whenever you make subsequent edits.

### C. Conversational Dialogue Hygiene
- Dialogue is the primary collaboration medium: explain your decisions, guide the user through what was accomplished, present next steps, and ask all questions directly in `dialogue`.
- Markdown (lists, bold formatting, inline code, and ```text ASCII flowcharts) is fully supported and encouraged in `dialogue`.
- NEVER dump large raw files, multi-file code blocks, or 100+ line documents into the `dialogue` field; those belong in `artifact`.
- CRITICAL FORBIDDEN: Never put conversational questions, greetings, or interactive scoping agendas in the `artifact`. Those belong strictly in the chat conversation.
