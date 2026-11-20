# Projects & Workspace Specifications

Specifications covering project registration, local script supervision, Git/Forge collaboration, multidisciplinary planning, file-backed knowledge workflows, and greenfield project creation.

## Index

| Spec | Title | Summary |
| :--- | :--- | :--- |
| `01` | [Project Registry, Scripts & Service Orchestration](01-project-registry-and-service-orchestration.md) | SQLite project registry, `rula.toml` script declarations, `ProcessSupervisor` namespaces, `ONESHIP_RUN_ID` boot reconciliation, and Scripts tab. |
| `02` | [Git Workspace, Branches & Snapshots Cockpit](02-git-workspace-and-branches.md) | Dual-signification Git status model, `rula.toml` VCS action presets, unified Git & Snapshots cockpit, diffs, and branch lifecycle. |
| `03` | [System Logging & Status Terminal](03-system-logging-and-status-terminal.md) | Cyclic rotating `oneship.log`, bounded SQLite `system_logs` sink with incremental vacuum pruning, and the 5-line rolling status terminal. |
| `04` | [Remote Repositories & Diagnostics](04-remote-repositories-and-diagnostics.md) | Zero-credential remote discovery/creation/cloning (`gh`, `glab`, direct Git), `GitRemote` classification, host diagnostics, and `pkexec` installer. |
| `05` | [Forge Provider Abstraction](05-forge-provider-abstraction.md) | Provider-neutral `ForgeProvider` contract, `project/{milestones,board,schedule,gantt,forge}` surface tree, `project.last_tasks_node`, and GitLab adapter. |
| `06` | [Forge Inbox & Triage](06-forge-inbox-and-triage.md) | SQLite `forge_inbox_items` read/unread/updated semantics, triage flags (`flagged`, `muted`), SSE notifications, and Forge filters/badges. |
| `07` | [Project Management & Milestones](07-project-management-and-milestones.md) | Opt-in `Project` vs `Journal` boundary, customizable Kanban stages, Schedule/Gantt geometry, `Fork ↔ Upstream` flow, and strategic milestones. |
| `08` | [Live Artifacts & Scratchpad](08-live-artifacts-and-scratchpad.md) | File-backed `docs/` and `scratchpad/` lifecycle, topical conversations, inline review comments, sandboxed Explorer ops, and CodeMirror 6 editor. |
| `10` | [Greenfield Project Creation](10-greenfield-project-creation.md) | Dual-path `AddProjectModal`, `create_greenfield_project` pre-boot, spec-first `greenfield_initializer` protocol, and ephemeral idle continuity. |

> [!NOTE]
> Spec number `09` is intentionally unassigned in this directory to preserve stable cross-links to [Greenfield Project Creation](10-greenfield-project-creation.md).

## Recommended Reading Order

1. **Core Workspace & Supervision:** [01](01-project-registry-and-service-orchestration.md) → [03](03-system-logging-and-status-terminal.md) → [10](10-greenfield-project-creation.md)
2. **Version Control & Forge Collaboration:** [02](02-git-workspace-and-branches.md) → [04](04-remote-repositories-and-diagnostics.md) → [05](05-forge-provider-abstraction.md) → [06](06-forge-inbox-and-triage.md)
3. **Planning & Knowledge Workspace:** [07](07-project-management-and-milestones.md) → [08](08-live-artifacts-and-scratchpad.md)
