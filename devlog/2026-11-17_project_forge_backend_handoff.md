# 2026-11-17 — Project/Forge Frontend Handoff to Backend

## Status

The frontend work for `Project` (`Tasks | Forge`, Board/Schedule/Gantt, global `SubtabBar`) is complete. No Rust or daemon code was changed in this session, and no build/test/runtime command was run by the agent.

The remaining work is the daemon-side migration that removes the temporary frontend mapping and makes Forge provider-neutral. It is specified in `docs/specs/39_forge_provider_and_project_surface_handoff.md`; the actionable list is in `TODO.md` under **Backend Handoff — Forge Provider & Project Surface (Spec 39)**.

## Current frontend state (implementation truth)

- `ProjectManagementTab.vue` presents two modes inside the existing `Project` tab:
  - `Tasks`: `board`, `schedule` and `gantt`.
  - `Forge`: `GithubCollabPanel` when the project has a remote, otherwise a link-repository empty state.
- The daemon surface tree is still:
  - `project/board`
  - `project/timeline` (labelled Timeline; rendered as Schedule or Gantt)
  - `project/github` (`issues`, `prs`, `releases`)
- The Gantt choice is stored per project in `localStorage` (`oneship:project:gantt_mode:<projectId>`).
- `lastTasksView` is a local ref; Forge → Tasks restores it.
- Forge is visible for any git project (`has_git`); the GitHub panel is used when `git_status.has_remote` is true, otherwise the empty state offers `Link Repository` (the existing multi-provider Remote VCS modal).
- `SubtabBar` is the global 36 px two-zone bar (tabs left, optional tools right).
- Specs: 13 `1.3.1`, 34 `0.2.4`, 35 `0.2.3`, 38 `0.2.2` (planned inbox), 39 `0.2.0` (planned backend handoff).

## Known gaps recorded as debt

- **#41** Forge provider adaptation: GitHub adapter only; GitLab collaboration missing.
- **#42** Gantt is frontend-local, not a surface node.
- **#43** Forge inbox/read-unread flags (Spec 38) not implemented.
- **#44** Git remote discovery is origin-centric (`has_remote` only origin/branch upstream).

## Backend work packages (summary)

1. **WP1 — Surface tree & last view:** `project/timeline` → `project/schedule`, add `project/gantt`, `project/github` → `project/forge` (with `has_git`), migrate persisted paths, add `project.last_tasks_node`, update resolver tests/bindings, and delete the frontend-local Gantt mapping.
2. **WP2 — Remotes/provider:** `GitStatus.remotes` with `ForgeProviderKind`; broaden `has_remote`; update hydration, tests, ts-rs and Specs 09/11.
3. **WP3 — Neutral Forge backend:** `kopeng/forge` types/trait/errors, GitHub adapter over the existing `*_blocking` functions, neutral `forge_*` commands and `project/forge` surface actions.
4. **WP4 — GitLab adapter:** `glab api` reads for issues/MRs/releases, typed provider/auth errors, fake-`glab` tests.
5. **WP5 — Inbox/flags:** provider-neutral Spec 38 implementation (`forge_inbox_items`, commands, event, surface actions, UI).
6. **WP6 — Docs/validation:** Specs 11/34/35/36/38, DT/TODO/devlog, bindings and requested tests.

Open decisions are listed in Spec 39 §13 and must be confirmed before coding (AGENTS.md §1.1). The main ones: node renames, neutral `forge_*` vs `github_*` commands, `glab` vs REST, provider-neutral inbox table, `last_tasks_node` slot, GitLab label policy, self-hosted provider override, and MCP tool naming.

## Prompt for the backend agent

Copy the following block into a new session in this repository:

```text
Trabajamos en ONE-SHIP (este repositorio). Antes de tocar código, lee y cumple AGENTS.md:
propuesta técnica y aprobación explícita antes de modificar archivos; prohibido ejecutar
comandos git; no ejecutar builds, tests, dev servers ni procesos de runtime salvo que el
usuario lo pida; usa ./scripts/run_dev.sh export-bindings cuando cambies structs
#[ts(export)], api::API_COMMANDS o events::DAEMON_EVENT_TYPES.

Contexto:
- La UI del tab Project ya está implementada y registrada en
  devlog/2026-11-17_project_tasks_forge_schedule_gantt.md.
- El contrato backend pendiente está en
  docs/specs/39_forge_provider_and_project_surface_handoff.md. Léelo completo, junto con
  Specs 11, 34 §6, 35 §6.3, 38 y TECHNICAL_DEBT #41/#42/#43/#44.
- El estado actual usa un mapeo temporal: project/timeline renderiza Schedule/Gantt y
  project/github renderiza Forge; Gantt se guarda en localStorage y el último view de
  Tasks vive en un ref local. Hay que eliminar ese mapeo con nodos reales del daemon.

Objetivo (Spec 39):
1. WP1: renombrar project/timeline -> project/schedule; agregar project/gantt; renombrar
   project/github -> project/forge (hijo de project, NO un tab de primer nivel) con
   required_capability = has_git; migrar los active_node_path persistidos
   (timeline->schedule, github->forge) en load_from_db; agregar el slot
   project.last_tasks_node (enum board|schedule|gantt, default board); actualizar tests
   del resolver y bindings; limpiar el frontend (showGantt, lastTasksView, localStorage,
   tasksView) y usar useUiSurfaceNode con board|schedule|gantt|forge.
2. WP2: agregar GitStatus.remotes: Vec<GitRemote> y ForgeProviderKind; has_remote debe
   reflejar todos los remotes; actualizar hydration, tests, ts-rs y Specs 09/11.
3. WP3: crear kopeng/forge con tipos neutrales, trait ForgeProvider síncrono y ForgeError;
   el adaptador GitHub delega en las funciones *_blocking existentes; agregar comandos
   get_forge_context, list_forge_issues, get_forge_issue, list_forge_pull_requests,
   get_forge_pull_request, list_forge_releases, create_forge_draft_pull_request y
   take_forge_issue; registrar en API_COMMANDS/dispatch/DaemonCommands y el test espejo;
   mover las surface actions a project/forge.
4. WP4: adaptador GitLab con glab api para issues, merge requests y releases; mapear los
   campos a los tipos neutrales; errores tipados de provider/auth; tests con un glab falso.
5. WP5: implementar Spec 38 generalizado: forge_inbox_items con provider, comandos de
   inbox/read/flag, evento forge-inbox-updated, surface actions y badges/filtros/mark-read
   en el frontend.
6. WP6: actualizar Specs 11/34/35/36/38, devlog, DT/TODO; exportar bindings y ejecutar
   los tests que el usuario autorice.

Antes de implementar: presenta una propuesta técnica con las decisiones abiertas de
Spec 39 §13 (renombres de nodos, comandos neutrales forge_* vs github_*, glab vs REST,
inbox neutral, last_tasks_node, etiquetas de GitLab, override self-hosted, MCP github_* vs
forge_*) y espera aprobación.

Restricciones: sin tab de primer nivel Forge; sin fallbacks silenciosos ni proveedor por
defecto; errores tipados (thiserror + ApiError); nada de std::fs/std::process::Command
dentro de async (usa spawn_blocking); cero dead code/backward compatibility; actualiza
specs y bindings en el mismo ciclo.

Entrega esperada: backend + ajustes frontend imprescindibles, tests, bindings exportados,
specs/devlog/TODO actualizados, y un reporte de lo verificado y lo que requiere runtime
manual.
```
