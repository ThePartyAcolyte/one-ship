# Development Log: Unified Live Thinking Bubble & Smart Pinned Auto-Scroll

**Date:** 2026-09-24  
**Topic:** Real-Time Reasoning Aggregation, Transient Chat Bubble Lifecycle, and Bi-Directional Smart Auto-Scroll  
**Build Version:** 0.2.1 (Pre-1.0 Baseline)

---

## 1. Problem Context & Requirements

1. **Terminal Log Shredding (OpenCode / DeepSeek):**
   - DeepSeek outputs internal thinking tokens in continuous small chunks (`agent_thought_chunk`).
   - The supervisor previously prepended `[THINKING]` and pushed a new line to the terminal ring buffer for every single token chunk, generating hundreds of 1-to-2 word lines and flooding the terminal view.
2. **Missing Antigravity Thought Extraction:**
   - Headless Antigravity (`agy --input-format stream-json --output-format stream-json`) emits `thought` step updates with fields like `thinking_delta` or `thinking`, but the supervisor discarded them with a static `[THINKING]` tag and generic `"Thinking..."` string.
3. **Chat Telemetry UX:**
   - The Chat Overlay featured a warning-style amber banner at the top (`bg-amber-500/10 text-amber-300`) rather than integrating naturally into the bottom of the conversation stream.
   - The user requested a unified, transient two-part bubble located at the bottom of the chat stream:
     - Upper section: Current thinking stream (rewrites/resets per thinking phase in multi-step turns).
     - Lower section: Subtle pulsing active process indicator for tools and system steps.
     - Entire bubble dissolves cleanly once the final response is delivered.
4. **Terminal & Chat Auto-Scroll Stalls:**
   - The terminal `<pre>` view in `FloatingChatOverlay.vue` had no auto-scroll handling, hiding new output as it arrived.
   - Chat history had no smart scroll-lock detection: it lacked awareness of whether the user had scrolled up to read history.

---

## 2. Implemented Architecture & Code Changes

### 2.1 Backend Reasoning Aggregation & Telemetry (`src-tauri/src/supervisor.rs`)
- **OpenCode Token Smoothing:**
  - Added `accumulated_thought`, `thought_line_buffer`, and `in_thought_phase` to the agent stdout reader task loop.
  - Buffered streaming `agent_thought_chunk` tokens, flushing to the terminal ring buffer only on newlines or word-boundary line lengths ($\ge 100$ characters).
  - Flushed remaining thought buffers cleanly when transitioning to `tool_call`, `agent_message_chunk`, or turn completion.
- **Antigravity (`agy`) Thought Parsing:**
  - Extracted `thinking_delta`, `thinking`, `text_delta`, or `text` from `step_update`.
  - Aggregated chunks and normalized stream handling to match OpenCode.
- **Atomic Telemetry Event (`agent-step-update`):**
  - Emitted `thought_text: accumulated_thought` in real time.
  - Retained thought text during subsequent tool calls and reset `accumulated_thought` when a new thinking phase begins.

### 2.2 Frontend Store (`src/stores/dialogueStore.ts`)
- Augmented `AgentLiveStep` interface with optional `thoughtText?: string`.
- Updated `agent-step-update` listener to retain and update `thoughtText`.
- Cleared active step state on turn completion and response delivery.

### 2.3 UI & Smart Auto-Scroll (`FloatingChatOverlay.vue` & `ConversationPanel.vue`)
- **Removed Top Banner:** Removed the top amber warning-style bar in `FloatingChatOverlay.vue`.
- **Transient Working & Thinking Bubble:**
  - Mounted before `<div ref="messagesEnd" />` in both chat components.
  - Monospace, brutalist styling using dynamic CSS variables (`--accent-color`, `--border-color`, `--dim-color`, `--fg-color`).
  - Section 1: Monospace thought box with internal scroll (`max-h-44`) displaying live thinking text.
  - Section 2: Subtle pulsing indicator (`w-1.5 h-1.5 rounded-full bg-[var(--accent-color)] animate-pulse`) with truncated active action summary.
  - Automatically unmounts when turn completes.
- **Smart Pinned Auto-Scroll:**
  - Implemented `@scroll` detection tracking whether the user is within 45px of the bottom (`isChatNearBottom`, `isTerminalNearBottom`).
  - Prevented scroll hijacking when the user scrolls up to review history.
  - Re-engaged auto-scroll automatically when the user scrolls back to the bottom.

---

## 3. Verification & Compliance Matrix

| Check | Target | Result |
|---|---|---|
| Rust Backend Check | `cargo check --bins` | Passed (0 errors, 0 warnings) |
| Rust Unit Tests | `cargo test --lib` | Passed (30/30 tests green) |
| Frontend TypeScript & Build | `npm run build` (`vue-tsc --noEmit && vite build`) | Passed (0 errors, 0 warnings) |
| Living Specification | `docs/specs/18_mcp_dialogue_channel_and_chat_overlay.md` | Updated (Phase 2G documented) |
