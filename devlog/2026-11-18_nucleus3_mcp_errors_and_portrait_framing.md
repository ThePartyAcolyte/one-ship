# 2026-11-18 — Nucleus 3: Prescriptive MCP Errors & Square Portrait Framing

## Summary
Audited the conversation/minutes/snapshot flow, standardized the remaining non-prescriptive MCP error in `mcp.rs` (Debt #25), and updated `AppAgentPortrait.vue` to render `1:1` square portraits anchored to the bottom of the `64×96` sprite tiles (Spec 21).

## Changes
1. **Prescriptive MCP Error Messaging (`src-tauri/src/mcp.rs`):**
   - Replaced `"The removal failed again. Fix these failures, then call complete_project_removal again: ..."` at `mcp.rs:4510` with a strictly prescriptive message directing the agent to resolve the filesystem error inside the project directory or report the blocker.
   - Removed resolved Debt #25 from `TECHNICAL_DEBT.md`.
2. **Square Bottom-Anchored Portrait Framing (`src/components/ui/AppAgentPortrait.vue`, `src/components/modals/AddPersonaModal.vue`, `docs/specs/21_agent_pixel_portrait_dna.md`):**
   - Changed `AppAgentPortrait.vue` from `2:3` (`64×96`) to `1:1` (`64×64` canvas), cropping `y = 32..96` (`CROP_TOP_OFFSET_PX = 32`) from each `64×96` sprite tile so the portrait is anchored to the base of the frame.
   - Replaced `bg-black/40` in `AppAgentPortrait.vue` and `bg-black/30` in `AddPersonaModal.vue` with `ui-surface-list` / `ui-surface-card`.
   - Deferred the visual layer customizer (Spec 21 §6) per user decision; randomizer + DNA string input remain active.
3. **Architecture & Spec Audit (`docs/WORK_PLAN.md`):**
   - Verified `snapshots.rs:403` already maps `agent_conversations.id` to `native_session_id` via `db_latest_native_session_for_conversation`.
   - Set aside `brief.rs` `reopened_from` chaining for separate review.

## Verification
- Read-only check via `./scripts/run_dev.sh status`.
