---
name: oneship-delegation
description: Stack-agnostic inter-agent delegation protocol, dynamic specialist discovery, task handover, and closed-loop callbacks.
version: 0.2.7
---

# ONE-SHIP Multi-Agent Delegation Protocol

This skill establishes the rules for task delegation, boundary respect, cross-project coordination, and closed-loop callbacks between domain specialists configured in ONE-SHIP.

---

## 1. Local Domain Specialists vs Generic Subagents

When operating inside ONE-SHIP:
- **FORBIDDEN:** Do not invoke unmanaged external subagents (`invoke_subagent`) to perform tasks that fall within the scope of an existing project specialist.
- **MANDATORY:** Delegate domain-specific work to the configured specialist agents using `delegate_agent_task`.

---

## 2. Dynamic Discovery (Projects, Specialists & Tasks)

Specialist personas, owned file boundaries, services, and tasks are dynamically configured in each project's `rula.toml`.

To discover projects, specialists, and available scripts without guessing:
1. **Consolidated Discovery (`discover_project`):** Inspects project metadata, Git status, registered specialist personas (with roles and live statuses), persistent services, and finite tasks in a single call. If no target project is specified, it automatically resolves to your current project.
2. **Project Directory (`list_projects`):** Lists all registered projects in ONE-SHIP with their configured agent personas.
3. **Agent Introspection (`query_project_agents`):** Inspects active specialists, system prompts, boundaries, and supervisor statuses for a specific project.

---

## 3. The Delegation Lifecycle (Closed-Loop & Non-Disruptive)

```text
 [ Originator Agent ]                                      [ Target Specialist ]
         │                                                           │
         ├── 1. delegate_agent_task ────────────────────────────────>│
         │   (Auto-infers origin; enqueues non-disruptively if busy) │
         │                                                           ├── 2. Executes task in pane
         │                                                           │   (verifies within its domain)
         │                                                           │
         │<── 3. Asynchronous Callback Notice ───────────────────────┼── 3. Calls report_task_status
         │   (Platform routes callback back to originator)           │
         ▼                                                           ▼
 (Resumes workflow)                                            (Dispatches next queued turn)
```

### Step 1: When to Delegate
Delegate a task when:
- The required file modification or domain logic falls strictly inside another specialist's declared directory boundary.
- A multi-stage workflow requires specialized execution (e.g. database migration, frontend redesign, or version control operations).
- You require services or actions in another project registered in ONE-SHIP.

### Step 2: Dispatching a Task (`delegate_agent_task`)
1. Formulate a self-contained, actionable instruction specifying:
   - Target files, directories, or endpoints.
   - Expected behavior, data contracts, and constraints.
   - Any verification steps or tests to run.
2. Call `delegate_agent_task`. The platform automatically infers your originating identity and project context.
3. If delegating to a specialist in an external project, specify `target_project`. For local specialists within your own project, omit the target project.
4. If the target agent is currently busy, the platform queues your instruction in a non-disruptive FIFO buffer to prevent turn collision.
5. If your current work depends entirely on the outcome of the delegation, conclude your turn with `post_agent_response`. The platform will automatically resume or notify you when the target completes the task.

### Step 3: Executing a Delegated Task (`report_task_status`)
When receiving a delegated task:
1. Execute the requested operations strictly within your assigned domain.
2. Do **NOT** call `record_session_minutes` while executing a delegated task (the platform rejects it; only the originating/orchestrating agent consolidates collective session minutes and checkpoints).
3. When finished, call `report_task_status` with your execution status (`completed`, `blocked`, or `error`) and a concise summary of results.
4. The platform automatically associates the report with the pending delegation, updates the audit journal, routes the callback notice back to the originator agent (even across project boundaries), and dispatches the next queued turn if any.

---

## 4. What Does NOT Require Delegation

- **Read-Only Inspection:** Reading or searching files across directory boundaries for context is permitted for all agents without delegation.
- **Local Domain Tasks:** If a change is 100% inside your declared file ownership, execute it directly.
