---
name: oneship-devlog
description: Database-backed development journaling, session minutes continuity, active task lifecycle, and living specification synchronization.
version: 0.2.7
---

# ONE-SHIP Development Journaling & Knowledge Graph Protocol

This skill establishes the rules for recording development milestones, managing active tasks, inspecting historical decisions, and keeping living specifications synchronized in ONE-SHIP.

---

## 1. Database-First Journaling Policy

In ONE-SHIP repositories:
- **FORBIDDEN:** Do not create or update flat markdown log files (`devlog/`, `TODO.md`, `TECHNICAL_DEBT.md`) on disk.
- **MANDATORY:** Persist all development minutes, decisions, and tasks directly into the project's SQLite Knowledge Graph via ONE-SHIP MCP tools.

---

## 2. When to Record Session Minutes (`record_session_minutes`)

Invoke the MCP tool `record_session_minutes` in the following scenarios:
1. **Milestone Completion & Code Verification:** Whenever you implement a feature, fix a bug, or modify files.
2. **Architectural Transition:** When introducing new patterns, data contracts, or breaking transitions.
3. **Session Handoff:** At the conclusion of an active development turn to summarize progress for the next agent or developer.

Minutes are attached automatically to your current conversation; never pass conversation or session identifiers.

**Conversation summary:** when the topic of the current conversation is concluded (or the user is about to clean the context), also pass `conversation_summary`: a few sentences with the topic, the outcome and any open points. It replaces any earlier summary and is what a future session is briefed with, so write it for someone who has not seen this conversation.

**Finding and reopening earlier conversations:** when the user wants to continue an earlier topic ("let's resume the database topic"), call `search_conversations(query)` with words describing it, show the user the best matches (topic, summary) if more than one is plausible, then call `reopen_conversation(conversation_id)`. The result is a Brief of that conversation: treat it as context, not a replay, and re-read any file before relying on it. Both tools act on your own conversations only; never pass project or agent identifiers.

**Topic changes:** if the user starts working on something clearly unrelated to the current conversation, say so briefly and suggest cleaning the context (the user decides). Before the context is cleaned, record the minutes with a `conversation_summary` so the topic can be found and reopened later.

> **Delegated Specialists:** If you are executing a delegated task from another agent (a pending `delegate_agent_task` assignment), do **NOT** call `record_session_minutes`—report your work via `report_task_status` so the primary/orchestrating agent records the collective session minutes.
>
> **Automatic Milestone Checkpoint (`[plugins.vcs].minutes_checkpoint_mode`):** Depending on project/global settings, calling `record_session_minutes` automatically creates either a hidden Git safety snapshot (`milestone:<id>`), a real Git commit on the active branch (subject = concise `title`; fails and rolls back the minute if Git hooks reject it), or no checkpoint (`off`). Keep `title` concise (<= 72 chars) and single-line.

---

## 3. Inspecting Historical Minutes & Decisions (`query_session_journal`)

To maintain context across sessions, retrieve previous decisions, or inspect files touched:
- Call `query_session_journal(limit=30)` to inspect recent minutes recorded by all agents in the project.
- Filter by `agent_id` (e.g. `query_session_journal(agent_id="backend")`) when investigating domain-specific history.

---

## 4. Active Task Lifecycle (`manage_todo_item`)

Active tasks are tracked directly in the database and rendered in the ONE-SHIP UI Task Panel:
1. **Creating Tasks:** When identifying pending work, prerequisites, or follow-up items, use `manage_todo_item(action="create", ...)`.
2. **Completing Tasks:** As soon as an item is implemented and verified, mark it completed via `manage_todo_item(action="complete", todo_id=...)` or `manage_todo_item(action="update_status", todo_id=..., status="completed")`.
3. **Pruning Inactive Tasks:** Deleted or obsolete tasks are removed via `manage_todo_item(action="delete", todo_id=...)`.

---

## 5. Living Specification Synchronization (`docs/specs/`)

- **Code Follows Specs:** Specifications in `docs/specs/` represent the formal contract of capabilities and architecture.
- **Atomic Synchronization:** If an implementation alters an IPC channel, data model, or workflow contract, the corresponding specification document in `docs/specs/` MUST be updated in the exact same turn.
