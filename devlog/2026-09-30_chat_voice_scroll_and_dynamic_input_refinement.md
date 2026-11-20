# Development Log: Chat Voice Auto-Scroll, Dynamic Floating Input & View Toggle Scroll Restoration

**Date:** 2026-09-30  
**Topic:** Chat UI Ergonomics, Voice Dictation Streaming Viewport Tracking, Dynamic Height Expansion, and Terminal/Messages Scroll Restoration

---

## 1. Overview & Context

During continuous voice transcription with OpenDictate (streaming acoustic hypotheses via `opendictate_interim_text`), the chat prompt input remained locked at line 1 (`scrollTop = 0`), preventing users from seeing incoming transcribed speech in real time. Additionally, the chat box was constrained to a single line height (`:rows="1"` with `resize-none`), requiring manual expansion capabilities. Finally, toggling between the terminal stream view and messages view in `FloatingChatOverlay` caused the message container to unmount and reset to `scrollTop = 0` (top of chat history).

---

## 2. Changes Implemented

### A. UI Component Library (`src/components/ui/AppTextarea.vue`)
- Added optional props:
  - `autoGrow?: boolean` (default: `false`): Enables dynamic calculation of `scrollHeight`.
  - `maxRows?: number` (default: `10`): Max line ceiling before locking height and enabling internal `overflow-y-auto`.
  - `autoScroll?: boolean` (default: `false`): Automatically synchronizes `scrollTop = scrollHeight` whenever `modelValue` changes.
- Implemented `adjustHeight()` using `window.getComputedStyle` for line height, padding, and border bounding.
- Implemented `scrollToBottom()` ensuring viewport tracks newly ingested tokens in real time.
- Exposed `adjustHeight` and `scrollToBottom` via `defineExpose`.

### B. Floating Chat Overlay (`src/components/dialogue/FloatingChatOverlay.vue`)
- **Scroll Preservation on View Toggle:**
  - Switched view containers from `v-if="chatViewMode === 'terminal'"` / `v-else` to `v-show="chatViewMode === 'terminal'"` and `v-show="chatViewMode === 'messages'"`.
  - Tracked `savedMessagesScrollTop` inside `onMessagesScroll`.
  - In `watch([chatViewMode, agentId])`, when returning to `'messages'`, restored exact `savedMessagesScrollTop` if scrolled up, or invoked `scrollToBottom(true)` if near bottom.
- **Floating Overlay Expansion Card:**
  - Converted the footer into a bottom-anchored floating composer card (`absolute bottom-0 left-0 right-0 z-20` with `bg-[var(--surface-color)]` and shadow) over a base spacer (`min-h-[50px]`).
  - Enabled `:auto-grow="true"`, `:auto-scroll="true"`, and `:max-rows="10"` on `AppTextarea`.
  - Pinned action buttons (Paperclip, Send) to `self-end h-[34px] min-h-[34px]` along the input baseline.
  - Repositioned the Microphone indicator to `bottom-2.5 right-2.5`.

### C. Docked Conversation Panel (`src/components/dialogue/ConversationPanel.vue`)
- Mirrored the floating expanding composer card architecture, bottom-pinned action controls, and bottom-right microphone positioning for full architectural consistency across docked and floating chat modes.

### D. OpenDictate Voice Auto-Send Integration
- **Rust Backend (`src-tauri/src/kopeng/opendictate.rs`):**
  - Added asynchronous and synchronous helpers (`check_opendictate_autosend` / `check_opendictate_autosend_sync`) inspecting `/tmp/opendictate_state.json` (`autosend_enabled` / `config.auto_send`) with fallback for future native D-Bus properties.
  - Extended `SessionFinishedPayload` with `pub auto_send: bool`. Emits resolved auto-send status in `opendictate_session_finished` event.
  - Added `pub autosend_enabled: bool` to `OpenDictateStatus` queried via `get_opendictate_status()`.
- **Dialogue & Voice Stores (`src/stores/`):**
  - Added `submitAgentPrompt(agentId, customText)` to `dialogueStore.ts`, unifying attachment packaging, comment context aggregation, draft clearing, and message dispatching.
  - Updated `voiceStore.ts` to track `isAutoSendEnabled`, refresh status via `refreshStatus()`, and automatically invoke `dialogueStore.submitAgentPrompt(targetAgent)` upon `opendictate_session_finished` when `auto_send` is active and final transcription is non-empty.
- **Microphone Tooltips:**
  - Integrated `voiceMicTooltip` across `FloatingChatOverlay.vue` and `ConversationPanel.vue`, displaying `[Auto-send]` badge when active in OpenDictate.

---

## 3. Verification & Validation

- `cargo check`: Rust backend compiled cleanly with zero errors (0.62s).
- `vue-tsc --noEmit && vite build`: Frontend TypeScript check and production bundle built successfully without errors (5.52s).
