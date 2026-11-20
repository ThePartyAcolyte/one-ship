# Voice and Capability Orchestration Specifications

This directory specifies ONE-SHIP's two Gemini Live voice subsystems—native dictation into focused
text fields and the global conversational orchestrator ("Ship Brain")—together with the Unified
Capability Core that bridges voice orchestration and internal MCP agents.

## Index

| Spec | Description |
| :--- | :--- |
| [Native Gemini Dictation](02-dictation-and-agent-chat.md) | Gemini Live speech-to-text into focused fields, revision-safe transcript reconciliation, contextual `Alt+V`, VAD rollover, reconnect replay, and auto-send. |
| [Global Voice Controller](03-global-voice-controller.md) | Push-to-Talk Gemini Live conversational orchestrator ("Ship Brain"), PipeWire audio bridge, credential isolation, and safety invariants. |
| [Unified Capability Core and Voice Orchestrator Harness](04-unified-capability-core-and-voice-harness.md) | Shared domain capability registry (`CapabilityProvider`), SQLite-backed persistent voice working sessions, and 65K TPM budget controls. |

## Recommended Reading Order

1. Start with [Native Gemini Dictation](02-dictation-and-agent-chat.md) for text-field dictation, the
   shared `Alt+V` shortcut, and Live-session recovery.
2. Continue with [Global Voice Controller](03-global-voice-controller.md) for the top-bar
   conversational orchestrator, PipeWire audio pipeline, and Gemini Live protocol constraints.
3. Read [Unified Capability Core and Voice Orchestrator Harness](04-unified-capability-core-and-voice-harness.md)
   for navigation-agnostic domain capabilities, persistent voice sessions, and TPM context management.
