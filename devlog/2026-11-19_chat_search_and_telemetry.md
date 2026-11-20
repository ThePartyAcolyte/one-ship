# 2026-11-19 — Chat Aesthetics, Conversation Search Modal & Runner/Model Token Telemetry

## Summary
1. **Chat Panel Aesthetics (`FloatingChatOverlay.vue`, Spec 13):**
   - Aligned chat header, agent toolbox, telemetry bar, message list, and composer to Spec 13 tokens (`.ui-surface-title`, `.ui-surface-list`, `.ui-surface-card`, `.ui-icon-btn`, `text-[8px]`/`text-[9px]`, semantic status/accent variables).
   - Removed all non-terminal `bg-black/*` and `text-[7px]` occurrences (leaving only the intentional Live Terminal Stream surface at `L1312`/`L1315`/`L1364`).
2. **Conversation Search & Context Transfer Modals (`ConversationSearchModal.vue`, `ContextTransferModal.vue`, `conversation.rs`, `db.rs`):**
   - Wrapped both modals in `<Teleport to="body">` so they escape the `z-20` / `z-40` stacking context of `FloatingChatOverlay` and render cleanly above `ProjectVerticalRail`, `AgentTaskbar`, and the chat resize handle.
   - Added `db_list_recent_conversations` and updated `conversation::search_conversations` so an empty query lists recent active/archived conversations instead of returning an error.
   - `ConversationSearchModal` now loads recent conversations automatically on open, supports `Escape` to close, and uses Spec 13 surfaces/controls.
3. **Runner & Model Telemetry Persistence (`db.rs`, `stats.rs`, `supervisor.rs`, `mcp.rs`, `events.rs`, `project_mgmt.rs`, `agentMetricsStore.ts`, `DevlogJournalView.vue`):**
   - Extended `agent_session_metrics` and `agent_session_journal` with `runner`, `model`, and `cost_dollars` (plus `max_context_tokens_count` on metrics).
   - `supervisor.rs` now populates `runner`, `model`, `max_context_tokens_count`, and `cost_dollars` on `agent-usage-update` for `opencode`, `claude`, and `agy`, and persists live usage via `db_upsert_live_session_usage`.
   - `stats::get_agent_metrics` merges persisted `agent_session_metrics` when local transcript files are unavailable, and `agentMetricsStore.ts` preserves live `agent-usage-update` values across polls.
   - `execute_record_session_minutes` computes per-minute deltas within a `conversation_id` (`db_sum_conversation_journal_usage`) to avoid double counting across multiple minutes in one conversation.
   - Added `RunnerModelUsageSummary` and `db_sum_project_usage_by_runner_model` to `ProjectMilestonesOverview`.
   - `DevlogJournalView.vue` displays `runner · model` and either USD cost (`$0.0000`) or token count (`tok`) per minute.
