# Spec 36: Global Voice Controller (`Gemini 3.8 Live Extended Thinking`)
**Document Version:** 0.2.1  
**Domain:** Hands-Free Conversational Orchestrator, Push-to-Talk Audio Bridge, Non-Blocking Tool Calling, Dynamic Surface Action Discovery  
**Maintainer:** ONE-SHIP / Omarchy Development Team

---

## 1. Architectural Purpose & Scope

ONE-SHIP introduces a global, project-independent conversational controller ("Ship Brain") powered by Google's **`models/gemini-3.8-live-extended-thinking`** over the `BidiGenerateContent` Live API. It replaces the legacy `system` agent concept without occupying a slot in per-project agent bars and builds directly on **Spec 35** (`UiStateManager` and `UiSurfaceRegistry`).

### 1.1 Legacy Top-Bar Cleanup
* The legacy `voiceMode` / `triggerVCSAction` / `coreStore.dictationSessionId` flow in `AppHeader.vue`, `App.vue`, `coreStore.ts`, and project card/tab props is **deleted** outright.
* The top-bar `Mic` button in `AppHeader.vue` is repurposed as the **Global Voice Controller** Push-to-Talk / toggle trigger.
* `OpenDictate` (`opendictate.rs`, `voiceStore.ts`) remains scoped strictly to local speech-to-text dictation inside `FloatingChatOverlay.vue`.

---

## 2. Model Profile & Protocol Constraints

* **Models (Configurable):**
  * `models/gemini-3.8-live` (**default**): Low-latency voice-agent model with interleaved reasoning and async `NON_BLOCKING` function calling (`thinkingConfig` omitted per protocol).
  * `models/gemini-3.8-live-extended-thinking` (optional): High-reasoning model using `thinkingConfig.thinkingLevel`.
* **Endpoint:** `wss://generativelanguage.googleapis.com/ws/google.ai.generativelanguage.v1beta.GenerativeService.BidiGenerateContent?key=$API_KEY`
* **Quota Profile (Free Tier):** Unlimited RPM / RPD, **65,000 TPM** ceiling.
  * *Token Budget Invariant:* System prompt, active project summary, and `setup.tools` declarations must stay compact (~2,000–3,000 tokens) so audio turns and tool responses remain well below 65K TPM.
* **`thinkingConfig`:** Configurable `thinking_level` (`"low" | "medium" | "high"`, default `"low"` when `gemini-3.8-live-extended-thinking` is selected; omitted for `gemini-3.8-live`).
* **Mandatory Non-Blocking Tools:** Every `functionDeclaration` in `setup.tools` **must** specify `"behavior": "NON_BLOCKING"`. Functions with zero arguments omit `parameters`; `surface_execute_action` accepts `params_json` (`STRING`).

---

## 3. Global Configuration & Credential Security

Added to global config (`[plugins.voice_controller]` in `config.rs` / `~/.config/one-ship/config.toml`):
* `enabled`: `bool` (default `true`; toggled from `AppHeader.vue` via the typed `set_voice_controller_enabled` daemon command, never via client-side TOML patching)
* `model`: `String` (`"gemini-3.8-live"` | `"gemini-3.8-live-extended-thinking"`, default `"gemini-3.8-live"`)
* `voice_name`: `String` (30 Gemini prebuilt studio voices, default `"Fenrir"`; testable via `test_voice_controller_voice`, which rejects unrecognized `model`/`voice_name`/`persona_style` values with `VoiceControllerError::InvalidConfig`)
* `thinking_level`: `String` (`"low" | "medium" | "high"`, default `"low"`)
* `persona_style`: `String` (`"belta" | "neutral"`, default `"belta"`)
* `gemini_api_key`: `Option<String>` (stored exclusively in daemon global config; **never** sent to or exposed in the Vue frontend)

Invalid types or unrecognized values present in `[plugins.voice_controller]` emit explicit `WARN` logs via `logger::log` rather than degrading silently.

---

## 4. Audio Bridge & Session Lifecycle

```text
  [Mic / Speaker]
        │ (Web Audio API: AudioWorklet PCM16 16kHz in / 24kHz out)
        ▼
  [Vue WebView] ◄── /api/events (ui-state-updated, ui-surface-command, voice-controller-state)
        │ (Local Daemon WebSocket: /api/v1/voice/stream)
        ▼
  [oneship-daemon: VoiceController] ──► [UiStateManager (Spec 35) / Supervisor / DB]
        │ (WSS BidiGenerateContent + API Key)
        ▼
  [Gemini 3.8 Live Extended Thinking]
```

1. **Push-to-Talk & PipeWire Synchronization Lifecycle:**
   * Pressing Push-to-Talk (`Alt+V` or `AppHeader.vue` `Mic` button) spawns `pw-record` (16 kHz PCM16 mono) and keeps the controller in `Connecting` until the **first PCM frame** is read from PipeWire, only then sending `activityStart` and transitioning to `Listening` so the UI waveform/indicator reflects genuine active capture.
   * Releasing / toggling off (`ptt_stop`) pre-warms `pw-play` (24 kHz PCM16 mono with a 100 ms silence pre-roll to prevent PipeWire sink startup clipping) and sends `activityEnd` while keeping playback open for the model's spoken response and async tool execution.
   * Audio output (24 kHz PCM) is played directly via `pw-play` in the daemon and downsampled waveform telemetry streams back over `/api/v1/voice/stream`, supporting immediate interruption (*barge-in*) on `ptt_start`.
2. **Bounded Buffers (Tiger Style):** Both input and output audio ring buffers in the daemon and frontend worklet are strictly bounded (`max_buffer_bytes` / `max_frames_count`); old unconsumed frames are dropped rather than growing unboundedly.

---

## 5. Two-Tier Action Protocol (Fixed Core + Dynamic Surface Discovery)

Because `BidiGenerateContent` fixes `setup.tools` at session initialization and the model has a 65K TPM cap, panel-specific tools are **never** enumerated upfront in `setup`. Instead, the controller uses a **Two-Tier Protocol** anchored to **Spec 35 (`UiSurfaceNode`)**:

### 5.1 Tier 1: Fixed `NON_BLOCKING` Tool Set in `setup`

**Navigation & UI State (Spec 35):**
* `ui_get_state()` — Returns the current `UiStateSnapshot` (`active_project_id`, `active_node_path`, `chat_panel_state`, `active_agent_id`) and the list of registered projects with their IDs/names/capabilities.
* `ui_select_project(project_id)` — Switches the active project via `UiAction::SelectProject`.
* `ui_navigate(project_id, node_path)` — Navigates to any `UiSurfaceNode` path (e.g. `"files"`, `"project/forge/prs"`, `"electronics/oscilloscope"`).
* `ui_open_agent_chat(project_id, agent_id, conversation_id?)` — Opens an agent's chat panel.
* `ui_minimize_agent_chat(project_id)` / `ui_restore_agent_chat(project_id)` / `ui_close_agent_chat(project_id)`.

**Agent & Project Orchestration:**
* `agent_launch(project_id, agent_id)` — Spawns/wakes an agent in the supervisor.
* `agent_stop(project_id, agent_id)` / `agent_interrupt(project_id, agent_id)` — Stops or interrupts an active agent.
* `agent_prompt(project_id, agent_id, message)` — Routes/delegates user instructions to a specific project agent.
* `agent_new_conversation(project_id, agent_id)` — Starts a fresh conversation thread for that agent.
* `agent_compact_context(project_id, agent_id)` — Dispatches the context compaction directive to the target agent.
* `project_search_skills(project_id, query)` — Searches installed/available skills for the project.

**Dynamic Surface Meta-Tools:**
* `surface_get_capabilities(project_id, node_path)` — Returns the declared `state_slots` and `actions` (`UiSurfaceActionDef`) for the requested `UiSurfaceNode`.
* `surface_execute_action(project_id, node_path, action_id, params)` — Executes a declared action on that panel.

### 5.2 Tier 2: Subsystem Action Declarations (`UiSurfaceActionDef`)

Spec 35's `UiSurfaceNode` is extended with `actions: Vec<UiSurfaceActionDef>`:

```text
UiSurfaceActionDef
├── id: String                  // e.g. "search_files", "open_file", "set_channel_enabled"
├── description: String         // Concise description for the model
├── execution_target: Backend | Frontend
└── parameters_schema: Value    // Compact JSON schema for parameters
```

* **`Backend` actions** (e.g. `files.search_files`, `files.list_directory`, `git.switch_branch`, `project.create_task`): Executed directly inside `oneship-daemon`, returning structured JSON in the `toolResponse` frame.
* **`Frontend` actions** (e.g. `files.open_file` updating `files.selected_document_path`, `electronics/oscilloscope.set_channel_enabled`, `electronics/oscilloscope.capture_single`): Dispatched via `DaemonEvent::ui_surface_command` over `/api/events` to the active Vue panel component.

### 5.3 Example Multi-Step Execution
1. *"Open the explorer and show me `ui_state.rs`":*
   * Model calls `ui_navigate(project_id, "files")` + `surface_get_capabilities(project_id, "files")`.
   * Model receives `search_files`, `list_directory`, `open_file`, calls `surface_execute_action(..., "search_files", { query: "ui_state.rs" })`, inspects the returned path list, and calls `surface_execute_action(..., "open_file", { path: "src-tauri/src/ui_state.rs" })`.
2. *"Open the oscilloscope and enable channel 1":*
   * Model calls `ui_navigate(project_id, "electronics/oscilloscope")` + `surface_get_capabilities(project_id, "electronics/oscilloscope")`.
   * Model receives oscilloscope actions and calls `surface_execute_action(..., "set_channel_enabled", { channel: 1, enabled: true })`.

---

## 6. Scoping & Safety Invariants (Tiger Style)

* **Zero Silent Fallbacks & Strict Tool Validation:** Every tool requiring `project_id` or `agent_id` must validate `project_id > 0` and resolve it unambiguously against registered projects (from `ui_get_state` or explicit user mention). `surface_execute_action` rejects malformed `params_json` with a structured error (no silent `{}` degradation) and propagates slot-persistence failures. All internal blocking helpers return typed `VoiceControllerError` rather than `Result<_, String>`. If the user references an agent name that exists across multiple projects without specifying one and `active_project_id` is ambiguous, the assistant must ask for verbal clarification instead of guessing.
* **Explicit Confirmation for Destructive Actions:** Actions such as `agent_stop` (while an agent is actively `working`) or destructive Git/file operations require explicit verbal confirmation before execution.
