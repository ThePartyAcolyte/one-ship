# Global Voice Controller

## Purpose

Defines the ONE-SHIP global, project-independent conversational controller ("Ship Brain") powered by Google's `models/gemini-3.8-live` and `models/gemini-3.8-live-extended-thinking` over the `BidiGenerateContent` Live API. It replaces the legacy `system` agent concept without occupying a slot in per-project agent bars, bridges Push-to-Talk PipeWire audio between the desktop and Gemini Live while isolating credentials in `oneship-daemon`, and builds on the UI State Machine (`UiStateManager` and `UiSurfaceRegistry`).

| Property | Value |
| :--- | :--- |
| Document version | 0.2.1 |
| Domain | Hands-free conversational orchestrator, Push-to-Talk audio bridge, non-blocking tool calling, dynamic surface action discovery |
| Maintainer | ONE-SHIP / Omarchy development team |

## Scope

In scope:

* Gemini Live model profiles, endpoint, `thinkingConfig`, 65,000 TPM token budget invariant, and mandatory `NON_BLOCKING` tool declarations.
* Global `[plugins.voice_controller]` configuration schema and OS-keyring credential isolation for the Gemini API key.
* Legacy top-bar dictation cleanup in `AppHeader.vue`, `App.vue`, `coreStore.ts`, and project card/tab props.
* Push-to-Talk (`Alt+V` / top-bar `Mic` button) PipeWire audio bridge (`pw-record` / `pw-play`), local WebSocket `/api/v1/voice/stream`, waveform telemetry, barge-in interruption, and bounded buffers.
* Baseline Two-Tier Action Protocol (`Tier 1` fixed tools + `Tier 2` `UiSurfaceActionDef` dynamic surface discovery) and Tiger Style scoping/confirmation invariants.

Out of scope:

* Native Gemini Live dictation into agent chat inputs and focused text fields (see [Native Gemini Dictation](02-dictation-and-agent-chat.md)).
* Navigation-decoupled domain capability registration, persistent SQLite working session harness, `contextWindowCompression`, and `sessionResumption` (see [Unified Capability Core and Voice Orchestrator Harness](04-unified-capability-core-and-voice-harness.md)).

## Model / Design

### Legacy Top-Bar Cleanup

* The legacy `voiceMode` / `triggerVCSAction` / `coreStore.dictationSessionId` flow in `AppHeader.vue`, `App.vue`, `coreStore.ts`, and project card/tab props is deleted outright.
* The top-bar `Mic` button in `AppHeader.vue` is repurposed as the Global Voice Controller Push-to-Talk / status trigger.
* Native dictation (`dictation.rs`, `dictationStore.ts`) remains scoped strictly to speech-to-text inside `FloatingChatOverlay.vue` and focused ONE-SHIP text fields.

### Model Profile and Protocol Constraints

* **Models (configurable):**
  * `models/gemini-3.8-live` (**default**): low-latency voice-agent model with interleaved reasoning and async `NON_BLOCKING` function calling (`thinkingConfig` omitted per protocol).
  * `models/gemini-3.8-live-extended-thinking` (optional): high-reasoning model using `thinkingConfig.thinkingLevel`.
* **Endpoint:** `wss://generativelanguage.googleapis.com/ws/google.ai.generativelanguage.v1beta.GenerativeService.BidiGenerateContent?key=$API_KEY`
* **Quota profile (Free Tier):** unlimited RPM / RPD, **65,000 TPM** ceiling.
  * *Token budget invariant:* the system prompt, active project summary, and `setup.tools` declarations must stay compact (~2,000–3,000 tokens) so audio turns and tool responses remain well below 65K TPM.
* **`thinkingConfig`:** configurable `thinking_level` (`"low" | "medium" | "high"`, default `"low"` when `gemini-3.8-live-extended-thinking` is selected; omitted for `gemini-3.8-live`).
* **Mandatory non-blocking tools:** every `functionDeclaration` in `setup.tools` **must** specify `"behavior": "NON_BLOCKING"`. Functions with zero arguments omit `parameters`; `surface_execute_action` (and its successor `capability_execute`) accepts `params_json` (`STRING`).

### Global Configuration and Credential Security

Stored in global config under `[plugins.voice_controller]` (`src-tauri/src/config.rs` / `~/.config/one-ship/config.toml`):

* `enabled`: `bool` (default `true`; toggled from `AppHeader.vue` via the typed `set_voice_controller_enabled` daemon command, never via client-side TOML patching).
* `model`: `String` (`"gemini-3.8-live"` | `"gemini-3.8-live-extended-thinking"`, default `"gemini-3.8-live"`).
* `voice_name`: `String` (30 Gemini prebuilt studio voices, default `"Fenrir"`; testable via `test_voice_controller_voice`, which rejects unrecognized `model` / `voice_name` / `persona_style` values with `VoiceControllerError::InvalidConfig`).
* `thinking_level`: `String` (`"low" | "medium" | "high"`, default `"low"`).
* `persona_style`: `String` (`"belta" | "neutral"`, default `"belta"`).
* Gemini API key: stored in the OS keyring (`secret-tool`, `service=one-ship`, `username=gemini_api_key`), cached in `oneship-daemon`, and **never** written to `config.toml` or exposed in the Vue frontend.

Invalid types or unrecognized values present in `[plugins.voice_controller]` emit explicit `WARN` logs via `logger::log` rather than degrading silently.

### Audio Bridge and Session Lifecycle

```text
  [Mic / Speaker]
        │ (PipeWire: pw-record PCM16 16kHz in / pw-play PCM16 24kHz out;
        │  WebView telemetry via WebSocket / SSE)
        ▼
  [Vue WebView] ◄── /api/events (ui-state-updated, ui-surface-command, voice-controller-state)
        │ (Local Daemon WebSocket: /api/v1/voice/stream)
        ▼
  [oneship-daemon: VoiceController] ──► [UiStateManager / Supervisor / DB]
        │ (WSS BidiGenerateContent + API Key)
        ▼
  [Gemini 3.8 Live / Extended Thinking]
```

*(Note: the original design envisaged Web Audio API `AudioWorklet` capture/playback in the WebView; the daemon implements direct PipeWire capture and playback via `pw-record` and `pw-play` while streaming 48-point downsampled waveform telemetry to the WebView over `/api/v1/voice/stream`.)*

1. **Push-to-Talk and PipeWire synchronization lifecycle:**
   * Pressing Push-to-Talk (`Alt+V` or the `AppHeader.vue` `Mic` trigger; `Alt+Backspace` / `Esc` to cancel) sends `ptt_start` over `/api/v1/voice/stream`. The daemon spawns `pw-record` (16 kHz PCM16 mono, 50 ms / 1,600-byte chunks) and keeps the controller in `Connecting` until the **first PCM frame** is read from PipeWire, only then sending `realtimeInput.activityStart` and transitioning to `Listening` so the UI waveform/indicator reflects genuine active capture.
   * Releasing / toggling off (`ptt_stop`) stops `pw-record`, pre-warms `pw-play` (24 kHz PCM16 mono with a 100 ms / 4,800-byte silence pre-roll to prevent PipeWire sink startup clipping), transitions to `Thinking`, and sends `realtimeInput.activityEnd` while keeping playback open for the model's spoken response and async tool execution.
   * Audio output (24 kHz PCM) is played directly via `pw-play` in the daemon, and 48-point downsampled waveform telemetry (`input_waveform`, `output_waveform`, `interrupted`) streams back over `/api/v1/voice/stream`, supporting immediate interruption (*barge-in*) on `ptt_start` or upstream `serverContent.interrupted`.
   * Active `org.mpris.MediaPlayer2.*` desktop media players currently `Playing` are paused on `ptt_start` via D-Bus (`org.freedesktop.DBus` / `gdbus`) and resumed when the voice turn returns to `Idle`, is cancelled, or disconnects.
2. **Bounded buffers (Tiger Style):** Both input and output audio queues/buffers in the daemon (`OUTBOUND_QUEUE_CAPACITY_COUNT = 128` frames) and frontend waveform buffers (`WAVEFORM_SAMPLES_COUNT = 48`) are strictly bounded; overflow chunks are dropped with `WARN` logs rather than growing unboundedly.

### Two-Tier Action Protocol (Fixed Core and Dynamic Surface Discovery)

Because `BidiGenerateContent` fixes `setup.tools` at session initialization and the model has a 65K TPM cap, panel-specific tools are **never** enumerated upfront in `setup`. Instead, the controller uses a **Two-Tier Protocol** anchored to the UI surface tree (`UiSurfaceNode`) and extended in [Unified Capability Core and Voice Orchestrator Harness](04-unified-capability-core-and-voice-harness.md):

#### Tier 1: Fixed `NON_BLOCKING` Tool Set in `setup`

**Navigation and UI state:**

* `ui_get_state()` — Returns the current `UiStateSnapshot` (`active_project_id`, `active_node_path`, `chat_panel_state`, `active_agent_id`) and the list of registered projects with their IDs, names, capabilities, and available tabs/domains.
* `ui_select_project(project_id)` — Switches the active project via `UiAction::SelectProject`.
* `ui_navigate(project_id, node_path)` — Navigates to any `UiSurfaceNode` path (e.g. `"files"`, `"project/forge/prs"`, `"electronics/oscilloscope"`).
* `ui_open_agent_chat(project_id, agent_id, conversation_id?)` — Opens an agent's chat panel.
* `ui_minimize_agent_chat(project_id)` / `ui_restore_agent_chat(project_id)` / `ui_close_agent_chat(project_id)` — Minimizes, restores, or closes the agent chat panel.

**Agent and project orchestration:**

* `agent_launch(project_id, agent_id)` — Spawns/wakes an agent in the supervisor.
* `agent_stop(project_id, agent_id)` / `agent_interrupt(project_id, agent_id)` — Stops or interrupts an active agent (with optional `corrective_prompt`).
* `agent_prompt(project_id, agent_id, prompt)` — Routes/delegates user instructions to a specific project agent.
* `agent_new_conversation(project_id, agent_id, topic?)` — Starts a fresh conversation thread for that agent.
* `agent_compact_context(project_id, agent_id)` — Dispatches the context compaction directive to the target agent.
* `project_search_skills(project_id, query)` — Searches installed/available skills for the project.

**Dynamic surface meta-tools (initial baseline design; superseded in runtime):**

* `surface_get_capabilities(project_id, node_path)` — Returns the declared `state_slots` and `actions` (`UiSurfaceActionDef`) for the requested `UiSurfaceNode`.
* `surface_execute_action(project_id, node_path, action_id, params_json)` — Executes a declared action on that panel.

*(In the current unified architecture, `surface_get_capabilities` and `surface_execute_action` have been replaced by navigation-agnostic `capability_discover` and `capability_execute` alongside `harness_scratchpad` and `harness_session_reset`; see [Unified Capability Core and Voice Orchestrator Harness](04-unified-capability-core-and-voice-harness.md).)*

#### Tier 2: Subsystem Action Declarations (`UiSurfaceActionDef`)

In the baseline design, `UiSurfaceNode` includes `actions: Vec<UiSurfaceActionDef>`:

```text
UiSurfaceActionDef
├── id: String                  // e.g. "search_files", "open_file", "set_channel_enabled"
├── description: String         // Concise description for the model
├── execution_target: Backend | Frontend
└── parameters_schema: Value    // Compact JSON schema for parameters
```

* **`Backend` actions** (e.g. `files.search_files`, `files.list_directory`, `git.switch_branch`, `project.create_task`): executed directly inside `oneship-daemon`, returning structured JSON in the `toolResponse` frame.
* **`Frontend` actions** (e.g. `files.open_file` updating `files.selected_document_path`, `electronics/oscilloscope.set_channel_enabled`, `electronics/oscilloscope.capture_single`): dispatched via `DaemonEvent::ui_surface_command` over `/api/events` to the active Vue panel component.

#### Example Multi-Step Execution (Baseline Surface Flow)

1. *"Open the explorer and show me `ui_state.rs`":*
   * Model calls `ui_navigate(project_id, "files")` + `surface_get_capabilities(project_id, "files")` (or `capability_discover(project_id, "files")` in the unified harness).
   * Model receives `search_files`, `list_directory`, `open_file`, calls `surface_execute_action(..., "search_files", { query: "ui_state.rs" })` (or `capability_execute`), inspects the returned path list, and calls `open_file` with `{ path: "src-tauri/src/ui_state.rs" }`.
2. *"Open the oscilloscope and enable channel 1":*
   * Model calls `ui_navigate(project_id, "electronics/oscilloscope")` + `surface_get_capabilities(project_id, "electronics/oscilloscope")` (or `capability_discover(project_id, "electronics")`).
   * Model receives oscilloscope actions and calls `set_channel_enabled` with `{ channel: 1, enabled: true }`.

### Scoping and Safety Invariants (Tiger Style)

* **Zero silent fallbacks and strict tool validation:** every tool requiring `project_id` or `agent_id` must validate `project_id > 0` and resolve it unambiguously against registered projects (from `ui_get_state` or explicit user mention). Action executors (`surface_execute_action` / `capability_execute`) reject malformed `params_json` with a structured error (no silent `{}` degradation) and propagate slot-persistence failures. All internal blocking helpers return typed `VoiceControllerError` rather than `Result<_, String>`. If the user references an agent name that exists across multiple projects without specifying one and `active_project_id` is ambiguous, the assistant must ask for verbal clarification instead of guessing.
* **Explicit confirmation for destructive actions:** actions such as `agent_stop` (while an agent is actively `working`) or destructive Git/file operations require explicit verbal confirmation before execution.

## Contracts

### Daemon Commands (`api.rs` / `daemonApi.ts`)

| Command | Arguments | Result | Purpose |
| :--- | :--- | :--- | :--- |
| `get_voice_controller_status` | `{}` (`NoArgs`) | `VoiceControllerStatus` | Returns current runtime state and non-secret config status (`configured`, `enabled`, `state`, `thinkingLevel`, `lastError`). |
| `set_voice_controller_enabled` | `{ enabled: boolean }` | `VoiceControllerStatus` | Atomically updates `[plugins.voice_controller].enabled` in `config.toml`, refreshes cache, and broadcasts `voice-controller-state`. |
| `test_voice_controller_voice` | `{ voiceName, model?, personaStyle? }` | `null` | Validates parameters (`InvalidConfig` on bad enum), opens an ephemeral Gemini Live session, and plays a 1-sentence greeting via `pw-play` (15 s timeout). |

### WebSocket Audio/Control Bridge (`/api/v1/voice/stream`)

* **Client-to-daemon JSON control frames:** `{ "action": "ptt_start" | "ptt_stop" | "ptt_cancel" | "disconnect" }`.
* **Daemon-to-client JSON telemetry frames:**
  * `{ "event": "input_waveform", "samples": number[48] }`
  * `{ "event": "output_waveform", "samples": number[48] }`
  * `{ "event": "interrupted" }`

### Daemon SSE Events (`/api/events`)

| Event | Payload Type | Trigger |
| :--- | :--- | :--- |
| `voice-controller-state` | `VoiceControllerStatus` | Any `VoiceControllerState` transition (`idle`, `connecting`, `listening`, `thinking`, `speaking`, `error`) or `set_voice_controller_enabled`. |
| `ui-surface-command` | `UiSurfaceCommandPayload` (`projectId`, `nodePath`, `actionId`, `params`) | Frontend-targeted surface/capability actions (`files.open_file`, `electronics/oscilloscope` controls). |

### Runtime State and Error Types (`src-tauri/src/voice_controller.rs`)

* **`VoiceControllerState`:** `idle` | `connecting` | `listening` | `thinking` | `speaking` | `error`.
* **`VoiceControllerError` (`ApiError`):** `Disabled` (`412`), `MissingApiKey` (`412`), `InvalidConfig` (`400`), `ProjectNotFound(i64)` (`404`), `Tool(String)` (`400`), `Io` (`500`), `WebSocketConnect(String)` (`502`), `Task(String)` (`500`).

### Configuration Keys (`[plugins.voice_controller]`, `scope = "global_only"`)

| Key | Type | Default | Allowed Values / Constraints |
| :--- | :--- | :---: | :--- |
| `enabled` | `boolean` | `true` | Toggled via `set_voice_controller_enabled`. |
| `model` | `select_enum` | `"gemini-3.8-live"` | `"gemini-3.8-live"`, `"gemini-3.8-live-extended-thinking"`. |
| `voice_name` | `select_enum` | `"Fenrir"` | 30 Gemini prebuilt studio voices (`GEMINI_LIVE_VOICES` in `config.rs`). |
| `thinking_level` | `select_enum` | `"low"` | `"low"`, `"medium"`, `"high"` (applied only on `gemini-3.8-live-extended-thinking`). |
| `persona_style` | `select_enum` | `"belta"` | `"belta"`, `"neutral"`. |
| `gemini_api_key` | `secret` | `""` | OS keyring only; excluded from `get_global_config` and never serialized to Vue frontend status payloads. |

*(Additional harness-specific keys under `[plugins.voice_controller]` are specified in [Unified Capability Core and Voice Orchestrator Harness](04-unified-capability-core-and-voice-harness.md).)*

## Implementation Status

| Item | Status | Evidence |
| :--- | :--- | :--- |
| `VoiceControllerManager`, `VoiceControllerError`, `VoiceControllerState`, `VoiceControllerStatus` | Implemented | `src-tauri/src/voice_controller.rs`, `src-tauri/src/types/generated/` |
| `get_voice_controller_status`, `set_voice_controller_enabled`, `test_voice_controller_voice` | Implemented | `src-tauri/src/api.rs`, `src-tauri/src/config.rs`, `src-tauri/src/voice_controller.rs`, `src/types/daemonApi.ts` |
| `/api/v1/voice/stream`, `pw-record` (16 kHz) / `pw-play` (24 kHz + 4,800-byte pre-roll), bounded 128-frame queue, 48-point waveform, MPRIS pause/resume | Implemented | `src-tauri/src/mcp.rs` (route), `src-tauri/src/voice_controller.rs`, `src/stores/voiceControllerStore.ts` |
| Top-bar oscilloscope pill, `Alt+V` PTT toggle, `Alt+Backspace`/`Esc` cancel, legacy top-bar flow removal | Implemented | `src/components/AppHeader.vue`, `src/App.vue`, `src/components/modals/SettingsModal.vue` |
| `[plugins.voice_controller]` schema (`global_only`) & `WARN` logging on invalid fields | Implemented | `src-tauri/src/config.rs` |
| Tier 1 `ui_*`, `agent_*`, `project_search_skills` `NON_BLOCKING` tools | Implemented (`ui_open_agent_chat` passes `conversation_id: None`; `agent_prompt` uses parameter `prompt`; `agent_new_conversation` accepts optional `topic`) | `src-tauri/src/voice_controller.rs` |
| Tier 2 `surface_get_capabilities` / `surface_execute_action` meta-tools | Superseded in runtime by `capability_discover` / `capability_execute` (`UiSurfaceActionDef` and `ui-surface-command` still exist in `ui_state.rs` and `events.rs`) | `src-tauri/src/voice_controller.rs`, `src-tauri/src/ui_state.rs`, `src-tauri/src/events.rs` |

## Related Specs

* [Native Gemini Dictation](02-dictation-and-agent-chat.md)
* [Unified Capability Core and Voice Orchestrator Harness](04-unified-capability-core-and-voice-harness.md)
* [Plugin Architecture (Kopeng)](../01-architecture/01-plugin-architecture.md)
* [Configuration Schema and Cascade](../01-architecture/02-configuration-schema-and-cascade.md)
* [README](README.md)

Supersedes legacy: `36_global_voice_controller.md`
