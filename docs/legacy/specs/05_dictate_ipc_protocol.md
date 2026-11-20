> [!WARNING]
> **LEGACY SPECIFICATION:** Superseded by [05-voice/02-dictation-and-agent-chat.md](../../specs/05-voice/02-dictation-and-agent-chat.md). Preserved for historical provenance only.

# OpenDictate IPC & D-Bus Communication Protocol Specification
**Document Version:** 1.3.0  
**Interface Name:** `org.kirulab.OpenDictate`  
**Standard Object Path:** `/org/kirulab/OpenDictate`  
**Target Bus:** D-Bus User Session Bus (`DBUS_BUS_SESSION`)  
**Secondary Transport:** Unix Domain Socket (`/tmp/opendictate.socket`)  
**Maintainer:** Kirulab / OpenDictate Project  

---

## 1. Overview & Architectural Contract

OpenDictate exposes a standardized inter-process communication (IPC) service designed for seamless integration with external third-party Linux applications (such as desktop note-taking apps, task managers, terminals, AI agents, and background daemons).

### 1.1 Zero Side-Effects Guarantee (Headless Capture)
When an external application initiates or reserves a capture session via D-Bus (`org.kirulab.OpenDictate`), OpenDictate operates under a strict **Zero Side-Effects Contract**:
1. **No Window Focus Hijacking**: OpenDictate will not steal, alter, or restore window focus to any external application.
2. **No Clipboard Tampering**: OpenDictate will not copy transcripts into the Wayland/X11 clipboard (`wl-copy` / `xclip`).
3. **No Keystroke Emulation**: OpenDictate will not simulate keyboard typing or `Enter` keypresses (`ydotool` / `wtype` / `xdotool`).
4. **Pure Data Signal Delivery**: Transcripts (raw speech and optional LLM-postprocessed text) are delivered exclusively through the `SessionFinished` D-Bus signal directly to the subscribing client.

---

## 2. Integration Models

OpenDictate supports two distinct capture workflows depending on client needs:

```
┌─────────────────────────────────────────────────────────────────────────────┐
│ 1. RESERVED / QUEUED CAPTURE (Decoupled User-Driven Workflow)              │
│                                                                             │
│ Client App                           OpenDictate             User Action    │
│    │                                      │                       │         │
│    ├─── ReserveCaptureSession(options) ──►│ (Arms mic & UI)       │         │
│    │◄── SessionReserved(uuid, ...) ───────┤                       │         │
│    │                                      │◄── Starts Speech ─────┤         │
│    │◄── SessionStarted(uuid) ─────────────┤                       │         │
│    │◄── InterimText(uuid, text) ──────────┤                       │         │
│    │                                      │◄── Finishes / Sends ──┤         │
│    │◄── SessionFinished(uuid, raw, text) ─┤                       │         │
│    │                                                                        │
└─────────────────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────────────────┐
│ 2. DIRECT CAPTURE (Imperative Client-Driven Workflow)                       │
│                                                                             │
│ Client App                           OpenDictate                            │
│    │                                      │                                 │
│    ├─── StartCaptureSession(options) ────►│ (Starts recording immediately)  │
│    │◄── SessionStarted(uuid) ─────────────┤                                 │
│    │◄── InterimText(uuid, text) ──────────┤                                 │
│    ├─── StopCaptureSession(uuid) ────────►│ (Stops & transcribes)           │
│    │◄── SessionFinished(uuid, raw, text) ─┤                                 │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 2.1 Model 1: Reserved / Queued Capture (`ReserveCaptureSession`) *(Recommended)*
* **Use Case**: An external application wants a voice note or dictated input, but leaves control of *when* to speak and *when* to finish to the user.
* **Flow**:
  1. Client calls `ReserveCaptureSession(options)` with a client-generated UUID, app name, and accent color.
  2. OpenDictate enters an **ARMED / RESERVED** state.
  3. The top bar widget and indicators dynamically adopt the client's `accent_color` and display `app_name`.
  4. The user starts dictation whenever ready using standard shortcuts (`SUPER+D`, bar click, or voice wake word).
  5. When the user finishes, OpenDictate emits `SessionFinished` to the client and automatically unarms itself.

### 2.2 Model 2: Direct Capture (`StartCaptureSession` / `StopCaptureSession`)
* **Use Case**: The external application has its own dedicated record/stop UI buttons or programmatic trigger.
* **Flow**:
  1. Client calls `StartCaptureSession(options)` $\rightarrow$ audio capture starts immediately.
  2. Client calls `StopCaptureSession(uuid)` $\rightarrow$ audio capture stops, STT runs, and `SessionFinished` is emitted.

---

## 3. Session Identification & Multi-App Eviction Protocol

### 3.1 Client UUIDs (`session_id`)
* Every session must be uniquely identified by a client-provided `session_id` string (UUID v4 is strongly recommended, e.g. `"f47ac10b-58cc-4372-a567-0e02b2c3d479"`).
* If a client passes an empty `session_id`, OpenDictate generates a UUID automatically and returns it in the method response.

### 3.2 Multi-Application Eviction Policy & In-Flight Capture Protection
1. **Idle Reservation Eviction (Last-Write-Wins):**
   - If Application B calls `ReserveCaptureSession` while Application A holds an idle, armed reservation (`state == IDLE` / `ARMED`):
   - OpenDictate detects session displacement (`session_id_A != session_id_B`).
   - OpenDictate immediately emits **`SessionCancelled(session_id_A)`** and **`SessionReservationReleased(session_id_A)`**.
   - Application A receives the signal and cleans up its UI state (resets mic watermark).
   - OpenDictate binds to Application B, adopts Application B's accent color, and emits **`SessionReserved(session_id_B, app_name, accent_color)`**.

2. **In-Flight Capture Protection (No Recording Hijacking):**
   - If Application B calls `ReserveCaptureSession` while OpenDictate is actively capturing audio (`state == RECORDING` / `PAUSED`):
   - OpenDictate **strictly protects the in-flight recording** and rejects/drops the incoming reservation request, returning an empty session ID or busy error.
   - Ongoing dictation in Application A is never interrupted, aborted, or hijacked mid-speech by background clicks or focus shifts in other applications.

---

## 4. Smart Double-Cancellation Routine

OpenDictate implements an intelligent two-tier cancellation policy for reserved sessions:

| User Action / Trigger | State When Cancelled | Daemon Behavior | Signals Emitted | Client Reservation Status |
|---|---|---|---|---|
| **Cancel Mid-Recording** (e.g. `opendictate --cancel`, noisy environment, verbal stumble) | `RECORDING` / `PAUSED` | Discards dirty audio buffer. Returns state to `IDLE`. | `InterimText(uuid, "")` | **REMAINS ARMED** (User can immediately press Record again to retry speech). |
| **Cancel in Idle** (e.g. user hits cancel without speaking) | `IDLE` (Armed) | Releases the client reservation entirely. Restores default system theme. | `SessionCancelled(uuid)`<br>`SessionReservationReleased(uuid)` | **RELEASED / CANCELLED** |
| **Client Release** (`ReleaseReservedSession`) | Any | Client explicitly drops its reservation. | `SessionReservationReleased(uuid)` | **RELEASED** |

---

## 5. D-Bus Interface Specification (`org.kirulab.OpenDictate`)

### 5.1 Methods

#### `ReserveCaptureSession(a{sv} options) -> (s session_id)`
Arms OpenDictate to deliver the next user-initiated dictation to the calling application.

* **Parameters:** `options` (`a{sv}` dictionary of variants):
  * `session_id` (`s`, optional): Client UUID. Generated if omitted.
  * `app_name` (`s`, optional): Human-readable name for UI badges and tooltips (default: `"App Externa"`).
  * `accent_color` (`s`, optional): Hexadecimal color string (`"#RRGGBB"`) applied to the microphone icon and live audio waveform while armed/recording (default: system theme accent).
  * `hide_bubble` (`b`, optional): If `true`, suppresses the external desktop GTK floating bubble/OSD window, allowing client applications to render real-time transcription and visual streaming indicators natively (default: `false`).
  * `ai_processing` (`b`, optional): Explicitly force (`true`) or disable (`false`) AI grammar/cleaning for this note.
  * `ai_prompt` (`s`, optional): Custom system prompt for LLM post-processing.
* **Returns:** `session_id` (`s`): The active session UUID.

#### `ReleaseReservedSession(s session_id) -> ()`
Releases an armed dictation reservation without recording.
* **Parameters:** `session_id` (`s`): The session UUID to release.

#### `StartCaptureSession(a{sv} options) -> (s session_id)`
Immediately begins headless microphone recording for the client.
* **Parameters:** `options` (`a{sv}`): Same dictionary options as `ReserveCaptureSession`.
* **Returns:** `session_id` (`s`): The active session UUID.

#### `StopCaptureSession(s session_id) -> ()`
Stops audio capture for the given session and triggers STT processing.
* **Parameters:** `session_id` (`s`): The active session UUID.

#### `CancelCaptureSession(s session_id) -> ()`
Cancels capture, discards audio buffers, and releases any armed reservation.
* **Parameters:** `session_id` (`s`): The session UUID to cancel.

#### `GetStatus() -> (s status_json)`
Retrieves real-time JSON daemon telemetry and state.
* **Returns:** `status_json` (`s`): JSON string. In `src-tauri/src/kopeng/opendictate.rs`, `get_opendictate_status` parses this payload alongside `/tmp/opendictate_state.json` (`autosend_enabled` / `config.auto_send`) into `OpenDictateStatus`:
  ```json
  {
    "state": "IDLE" | "RECORDING" | "PAUSED" | "TRANSCRIBING" | "CLEANING",
    "is_busy": false,
    "active_dbus_session": "oneship-bosmang-1794845899000" | null,
    "reserved_session": "oneship-bosmang-1794845899000" | null,
    "is_available": true,
    "autosend_enabled": false
  }
  ```

---

### 5.2 Signals

#### `SessionReserved(s session_id, s app_name, s accent_color)`
Emitted when an application successfully arms a reservation.

#### `SessionReservationReleased(s session_id)`
Emitted when a reservation is un-armed or explicitly released.

#### `SessionStarted(s session_id)`
Emitted when microphone recording begins for the session.

#### `SessionFinished(s session_id, s raw_text, s processed_text, s status)`
Emitted when speech processing finishes.
* `session_id` (`s`): The session UUID.
* `raw_text` (`s`): Direct transcript from Faster-Whisper or Gemini Live.
* `processed_text` (`s`): Post-processed text (or identical to `raw_text` if AI processing was disabled).
* `status` (`s`): Status code:
  * `"ok"`: Speech transcribed successfully.
  * `"empty"`: Audio captured was pure silence / no speech detected.

#### `SessionCancelled(s session_id)`
Emitted when a session is aborted, cancelled by the user, or evicted by another client.

#### `InterimText(s session_id, s interim_text)`
Emitted during active streaming recording with real-time speech hypotheses.
* `session_id` (`s`): The active session UUID.
* `interim_text` (`s`): The complete cumulative transcription buffer of the active utterance so far (not an incremental delta token). Clients can directly and atomically replace provisional speech text in their UI buffers.

---

## 6. Implementation Recipes & Code Examples

### 6.1 Python 3 Client (using `GIO` / `PyGObject`)

```python
import uuid
import gi
gi.require_version("Gio", "2.0")
gi.require_version("GLib", "2.0")
from gi.repository import Gio, GLib

class OpenDictateClient:
    def __init__(self, app_name="MyNotesApp", accent_color="#4CAF50"):
        self.app_name = app_name
        self.accent_color = accent_color
        self.bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
        self.current_session_id = None
        
        # Subscribe to signals
        self.bus.signal_subscribe(
            "org.kirulab.OpenDictate",
            "org.kirulab.OpenDictate",
            None,
            "/org/kirulab/OpenDictate",
            None,
            Gio.DBusSignalFlags.NONE,
            self._on_signal,
        )

    def reserve_voice_note(self, on_finished_callback, ai_processing=False, ai_prompt=None):
        """Arm OpenDictate for the next user voice note."""
        self.on_finished = on_finished_callback
        self.current_session_id = str(uuid.uuid4())

        opts = {
            "session_id": GLib.Variant("s", self.current_session_id),
            "app_name": GLib.Variant("s", self.app_name),
            "accent_color": GLib.Variant("s", self.accent_color),
            "ai_processing": GLib.Variant("b", ai_processing)
        }
        if ai_prompt:
            opts["ai_prompt"] = GLib.Variant("s", ai_prompt)

        self.bus.call_sync(
            "org.kirulab.OpenDictate",
            "/org/kirulab/OpenDictate",
            "org.kirulab.OpenDictate",
            "ReserveCaptureSession",
            GLib.Variant("(a{sv})", (opts,)),
            GLib.VariantType("(s)"),
            Gio.DBusCallFlags.NONE,
            2000,
            None,
        )
        print(f"[*] Voice note reserved (UUID: {self.current_session_id})")

    def cancel_reservation(self):
        """Release reservation if user aborts before speaking."""
        if self.current_session_id:
            self.bus.call_sync(
                "org.kirulab.OpenDictate",
                "/org/kirulab/OpenDictate",
                "org.kirulab.OpenDictate",
                "ReleaseReservedSession",
                GLib.Variant("(s)", (self.current_session_id,)),
                None,
                Gio.DBusCallFlags.NONE,
                2000,
                None,
            )
            self.current_session_id = None

    def _on_signal(self, conn, sender, path, iface, signal_name, params):
        data = params.unpack() if params else ()
        
        if signal_name == "SessionFinished":
            sid, raw_text, final_text, status = data
            if sid == self.current_session_id:
                print(f"[✔] Voice Note Delivered: '{final_text}' (status: {status})")
                if hasattr(self, "on_finished") and self.on_finished:
                    self.on_finished(final_text, raw_text, status)
                self.current_session_id = None

        elif signal_name == "SessionCancelled":
            sid = data[0] if data else ""
            if sid == self.current_session_id:
                print(f"[!] Session {sid} was cancelled or evicted.")
                self.current_session_id = None

        elif signal_name == "InterimText":
            sid, partial = data
            if sid == self.current_session_id:
                print(f"[~] Live Interim: {partial}", end="\r")
```

---

### 6.2 CLI & Shell Testing (`gdbus` / `busctl`)

#### Check Daemon Status
```bash
gdbus call --session \
  --dest org.kirulab.OpenDictate \
  --object-path /org/kirulab/OpenDictate \
  --method org.kirulab.OpenDictate.GetStatus
```

#### Arm a Reservation via `gdbus`
```bash
gdbus call --session \
  --dest org.kirulab.OpenDictate \
  --object-path /org/kirulab/OpenDictate \
  --method org.kirulab.OpenDictate.ReserveCaptureSession \
  "{'session_id': <'test-uuid-123'>, 'app_name': <'MyCliApp'>, 'accent_color': <'#E91E63'>, 'ai_processing': <false>}"
```

#### Monitor Live D-Bus Signals
```bash
busctl --user monitor org.kirulab.OpenDictate
```

#### Release an Armed Reservation
```bash
gdbus call --session \
  --dest org.kirulab.OpenDictate \
  --object-path /org/kirulab/OpenDictate \
  --method org.kirulab.OpenDictate.ReleaseReservedSession \
  "'test-uuid-123'"
```

---

## 7. Secondary Transport: Unix Domain Socket (`/tmp/opendictate.socket`)

For lightweight command-line triggers, scripts, and local keyboard bindings, OpenDictate provides a raw text Unix Domain Socket.

### Available Commands:
* `record`: Starts recording if IDLE, resumes if PAUSED, or stops/sends if RECORDING.
* `send`: Immediately stops recording, transcribes, and pastes into the active focused window.
* `cancel`: Discards audio recording or clears IDLE reservation.
* `pause`: Toggles pause/resume during active audio capture.
* `finish-normal`: Forces finish without AI rewriting.
* `finish-ai`: Forces Gemini AI post-processing.
* `toggle-ai`: Toggles global AI cleanup flag.
* `settings`: Opens the Settings UI.
* `wizard`: Opens the Initial Setup Wizard.
* `set-config <key>:<value>`: Modifies configuration parameter dynamically.
* `set-bar-position <left|center|right>`: Relocates top bar widget in Omarchy shell.

#### Socket Example (Bash / Netcat):
```bash
echo "record" | nc -U -u -q 0 /tmp/opendictate.socket
```
