# Forge Inbox & Triage

## Purpose

Define ONE-SHIP's provider-neutral inbox read/unread tracking and triage flagging layer (`forge_inbox_items`) over external Forge issues and pull/merge requests across GitHub and GitLab, persisting developer review state across sessions and surfacing it to the UI, MCP agents, and the Global Voice Controller.

## Scope

In scope:
- SQLite persistence schema (`forge_inbox_items`) and unique indexing.
- Read, new-activity (`updated`), unread (`new`), and triage-flag (`flagged`, `muted`) semantics.
- Daemon commands, `forge-inbox-updated` SSE event, `project/forge` surface/capability actions, and MCP tools.
- Frontend badge, filter, and mark-read integration in `ForgeCollabPanel.vue` and `forgeCollabStore.ts`.

Out of scope:
- Underlying `ForgeProvider` issue/PR fetching and GitLab/GitHub CLI adapters ([Forge Provider Abstraction](05-forge-provider-abstraction.md)).
- Remote repository search, cloning, and credential handling ([Remote Repositories & Diagnostics](04-remote-repositories-and-diagnostics.md)).

## Model / Design

### 1. Persistence Model (`forge_inbox_items`)

Inbox state is stored in `oneship.db` (`src-tauri/src/db.rs`), keyed uniquely by `(project_id, provider, repo, item_kind, item_number)`:

| Column | Type | Description |
| :--- | :--- | :--- |
| `id` | `INTEGER PRIMARY KEY` | Internal row ID |
| `project_id` | `INTEGER NOT NULL` | Foreign key to `projects(id)` (`ON DELETE CASCADE`) |
| `provider` | `TEXT NOT NULL` | `"github"` \| `"gitlab"` \| `"unknown"` |
| `repo` | `TEXT NOT NULL` | Repository path / `owner/name` |
| `item_kind` | `TEXT NOT NULL` | `"issue"` \| `"pr"` |
| `item_number` | `INTEGER NOT NULL` | Upstream issue number (GitHub) or IID (GitLab) |
| `provider_node_id` | `TEXT` | Optional upstream immutable node ID |
| `last_seen_updated_at` | `TEXT` | ISO-8601 `updated_at` timestamp when last marked read |
| `is_read` | `INTEGER NOT NULL DEFAULT 0` | `1` when explicitly marked read or opened |
| `triage_flag` | `TEXT` | Optional user triage flag: `"flagged"` \| `"muted"` \| `NULL` |
| `updated_epoch_seconds` | `INTEGER NOT NULL` | Local mutation timestamp |

Indexes:
- `idx_forge_inbox_unique` on `(project_id, provider, repo, item_kind, item_number)`
- `idx_forge_inbox_proj` on `(project_id, provider, is_read, triage_flag)`

### 2. Unread, New-Activity & Triage Semantics

Computed in `src-tauri/src/kopeng/forge/inbox.rs` by joining upstream issue/PR summaries against persisted `forge_inbox_items` rows:

- **Unread (`new`):** No row exists for `(project_id, provider, repo, item_kind, item_number)`, or `is_read = 0`.
- **New Activity (`updated`):** `is_read = 1` and upstream `item.updated_at > last_seen_updated_at`.
- **Read (`read`):** `is_read = 1` and `item.updated_at <= last_seen_updated_at`.

Triage flags (`ForgeTriageFlag`) support `"flagged"`, `"muted"`, or `null` (cleared). Upserting read state preserves `last_seen_updated_at` via `COALESCE(excluded.last_seen_updated_at, forge_inbox_items.last_seen_updated_at)`.

### 3. UI & Interaction Behavior

Implemented in `ForgeCollabPanel.vue`, `forgeCollabStore.ts`, and `ProjectManagementTab.vue`:
- **Unread Badges:** Displays unread counts on the `Issues` and `Pull Requests` / `Merge Requests` tabs of the single Project `SubtabBar`.
- **List Filters:** Each subview provides `All`, `Inbox` (items whose `inboxStatus !== 'read'`), and `Flagged` (`triageFlag === 'flagged'`) filters.
- **Row Indicators & Actions:**
  - Unread/updated items display `NEW` or `UPD` badges.
  - Per-row flag toggle (`flagged` / `null`) and explicit `Mark Read` / `Mark Unread` controls.
  - Opening an issue or PR/MR automatically marks it read using its upstream `updated_at` timestamp.
- **Zero Local Storage:** All read and flag state persists exclusively in SQLite via the daemon; no `localStorage` fallback is used.

## Contracts

### Exported Types (`src-tauri/src/kopeng/forge/inbox.rs`, `ts-rs`)

- `ForgeInboxStatus`: `"new" | "updated" | "read"`
- `ForgeTriageFlag`: `"flagged" | "muted"`
- `ForgeItemKind`: `"issue" | "pr"`
- `ForgeInboxItem`: `{ provider, repo, itemKind, itemNumber, title, state, author, url, updatedAt, inboxStatus, isRead, triageFlag }`
- `ForgeUnreadCounts`: `{ provider, issuesUnreadCount, prsUnreadCount, totalUnreadCount, updatedCount, flaggedCount }`

### Daemon Commands (`api.rs` & `src/types/daemonApi.ts`)

| Command | Arguments (`camelCase`) | Result |
| :--- | :--- | :--- |
| `list_forge_inbox` | `{ projectId, provider?, unreadOnly? }` | `ForgeInboxItem[]` |
| `mark_forge_item_read` | `{ projectId, provider?, itemKind, number, read, updatedAt? }` | `null` |
| `set_forge_item_flag` | `{ projectId, provider?, itemKind, number, flag }` | `null` |
| `get_forge_unread_counts` | `{ projectId, provider? }` | `ForgeUnreadCounts` |

### SSE Event (`events.rs` & `daemonEvents.ts`)

- **`forge-inbox-updated`** (`ForgeInboxUpdatedPayload`): `{ projectId, provider, unreadCount, updatedCount }`, emitted whenever `mark_forge_item_read` or `set_forge_item_flag` mutates state.

### Surface Actions & MCP Tools

- **Surface / Capability (`project/forge`, `capabilities/forge.rs`):**
  - `list_inbox` (`forge_list_inbox`)
  - `mark_item_read(item_kind, number, read)` (`forge_mark_item_read`)
  - `set_item_flag(item_kind, number, flag)` (`forge_set_item_flag`)
  - `get_unread_counts` (`forge_get_unread_counts`)
  - Accompanying read actions on the same node: `list_issues(state?, limit?)`, `get_issue(number)`, `list_prs(state?, limit?)`, `get_pr(number)`, `list_releases(limit?)`.
- All MCP invocations resolve project ownership from the attested caller identity (`CallerContext`) before reading or mutating `forge_inbox_items`.

## Implementation Status

| Area | Status | Notes |
| :--- | :--- | :--- |
| SQLite `forge_inbox_items` table & queries (`db.rs`) | Implemented | — |
| Inbox logic, commands & `forge-inbox-updated` event (`kopeng/forge/inbox.rs`, `events.rs`, `api.rs`) | Implemented | — |
| Capability actions & MCP tools (`capabilities/forge.rs`, `mcp.rs`, `ui_state.rs`) | Implemented | — |
| Frontend badges, `Inbox`/`Flagged` filters, mark-read on open (`ForgeCollabPanel.vue`, `forgeCollabStore.ts`) | Implemented | — |

## Related Specs

- [Forge Provider Abstraction](05-forge-provider-abstraction.md)
- [Project Management & Milestones](07-project-management-and-milestones.md)
- [Remote Repositories & Diagnostics](04-remote-repositories-and-diagnostics.md)
- [Project Category Index](README.md)

Supersedes legacy: `38_github_inbox_and_triage_flags.md`
