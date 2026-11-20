# Spec 38: Forge Inbox & Triage Flags
**Document Version:** 0.3.0  
**Domain:** Provider-Neutral Forge Collaboration State, Read/Unread Tracking, Triage Flags  
**Maintainer:** ONE-SHIP / Omarchy Development Team

---

## 1. Architectural Purpose & Scope

Adds a provider-neutral inbox-style read/unread and triage-flagging layer on top of Forge Issues and Pull/Merge Requests (Spec 34 / Spec 39), enabling developers and the Global Voice Controller (Spec 36) to track newly arrived or updated items across GitHub and GitLab without losing place across sessions.

---

## 2. Persistence Model (`forge_inbox_items`)

Stored in SQLite (`oneship.db`), keyed uniquely by `(project_id, provider, repo, item_kind, item_number)`:

| Column | Type | Description |
| :--- | :--- | :--- |
| `id` | `INTEGER PRIMARY KEY` | Internal row ID |
| `project_id` | `INTEGER NOT NULL` | Foreign key to `projects.id` |
| `provider` | `TEXT NOT NULL` | `"github"` \| `"gitlab"` \| `"unknown"` |
| `repo` | `TEXT NOT NULL` | Repository path / `owner/name` |
| `item_kind` | `TEXT NOT NULL` | `"issue"` \| `"pr"` |
| `item_number` | `INTEGER NOT NULL` | Upstream issue or PR/MR number |
| `provider_node_id` | `TEXT` | Optional upstream immutable ID |
| `last_seen_updated_at` | `TEXT` | ISO-8601 `updated_at` timestamp when last marked read |
| `is_read` | `INTEGER NOT NULL DEFAULT 0` | `1` when explicitly read or opened |
| `triage_flag` | `TEXT` | Optional user flag: `"flagged"` \| `"muted"` \| `NULL` |
| `updated_epoch_seconds` | `INTEGER NOT NULL` | Local mutation timestamp |

### Unread & New-Activity Semantics
- **Unread (`new`):** No row exists for `(project_id, provider, repo, item_kind, item_number)`, or `is_read = 0`.
- **New Activity (`updated`):** `is_read = 1` but upstream `item.updated_at > last_seen_updated_at`.
- **Read:** `is_read = 1` and `item.updated_at <= last_seen_updated_at`.

---

## 3. UI, Commands, Events & Surface Actions (`project/forge`)

1. **Daemon Commands:** `list_forge_inbox`, `mark_forge_item_read`, `set_forge_item_flag`, `get_forge_unread_counts`.
2. **SSE Event:** `forge-inbox-updated` (`{ projectId, provider, unreadCount, updatedCount }`).
3. **Visual Indicators:**
   - Unread count badges on the `Issues` and `Pull Requests` / `Merge Requests` tabs of the Project panel bar (`ProjectManagementTab.vue`, Spec 34 §6.5).
   - `Inbox` / `Flagged` filters, flag toggle (`flagged`), `Mark Read` / `Mark Unread` actions, and automatic mark-read on item open.
4. **Voice / Surface Actions on `project/forge`:**
   - `list_issues(state?, limit?)`, `get_issue(number)`
   - `list_prs(state?, limit?)`, `get_pr(number)`
   - `list_releases(limit?)`
   - `mark_item_read(item_kind, number, read)`
   - `set_item_flag(item_kind, number, flag)`
