# Agent Pixel Portrait DNA

## Purpose

Give every AI agent persona a deterministic, human-developer pixel portrait encoded as a compact DNA string (`B02:E0:H04:BD02:A05`) and composited on an HTML5 Canvas from transparent 64×96 layered PNG assets.

## Scope

In scope: asset filtering rules, 64×96 sprite geometry, `public/portraits/manifest.json` five-layer catalog, DNA string grammar, deterministic hashing (`src-tauri/src/kopeng/dna.rs`), `rula.toml` `[agents.<id>].dna` schema, `<AppAgentPortrait />` rendering (`src/components/ui/AppAgentPortrait.vue`), and UI integration points (`AgentTaskbar.vue`, `FloatingChatOverlay.vue`, `AddPersonaModal.vue`).

Out of scope: full persona configuration schema beyond `dna` (see [Configuration Schema and Cascade](../01-architecture/02-configuration-schema-and-cascade.md)) and general UI component design tokens (see [UI Component Library and Composition Recipes](../07-ui/02-ui-component-library-and-recipes.md)).

## Model / Design

### 1. Asset Library Sanitization & Catalog (`public/portraits/manifest.json`)

- **Filtering rules:** Non-human fantasy packs (`extras/Pixel-Portraits-Addon/Legacy-Pack/{Bird, Frog, Minotaur, Orc, Skeleton, Wolf}`) and non-human accessories (nose rings, tusk rings, skeleton items) are excluded; only human skin bases, expressions, hair styles/colors, beards, glasses, hats, bandanas, and chains are included.
- **Geometry:** Fixed `64×96` px tiles (single PNGs or composite sheets sliced on 64×96 boundaries).
- **Five ordered rendering layers (bottom to top):**
  1. `base`: `B01`–`B14` (masculine: `B01`–`B04`, `B07`–`B10`, `B13`–`B14`; feminine: `B05`, `B06`, `B11`, `B12`).
  2. `expression`: `E0` (default), `E1` (happy), `E2` (sad), `E3` (annoyed), `E4` (angry), `E5` (surprised).
  3. `beard`: `BD00` (none), `BD01`–`BD05`. Automatically suppressed (`BD00`) when the selected base is feminine.
  4. `hair`: `H00` (bald), `H01`–`H21`.
  5. `accessory`: `A00` (none), `A01`–`A16`.

### 2. DNA Encoding & Deterministic Generation

Colon-delimited format:

```text
BASE_ID : EXPR_ID : HAIR_ID : BEARD_ID : ACC_ID
```

Example: `B02:E0:H04:BD01:A03`.

- **Backend generation (`src-tauri/src/kopeng/dna.rs`):** `generate_random_dna(agent_id)` hashes `agent_id` deterministically (`DefaultHasher`), selects valid indices per layer while enforcing `BD00` for feminine bases, and persists `dna` under `[agents.<id>]` in `rula.toml` when initialized without one so the portrait remains stable across restarts.
- **Frontend rendering (`AppAgentPortrait.vue`):** Accepts `dna?: string | null`, `agentId?: string | null`, and `size?: number` (default `48`). If `dna` is omitted, `generateFallbackDna(agentId)` computes a deterministic fallback DNA. Composites the five PNG layers sequentially on HTML5 Canvas 2D using a bottom-anchored `64×64` crop (`y = 32..96`) of each `64×96` tile with `imageSmoothingEnabled = false` and CSS `image-rendering: pixelated;`.

### 3. UI Integration & Roadmap

- Rendered in `AgentTaskbar.vue`, `FloatingChatOverlay.vue`, and `AddPersonaModal.vue` (live preview + randomize button).
- **Roadmap (Phase 2):** Interactive Avatar Customizer Modal with visual layer pickers for manual fine-tuning of skin tone, hair, beard, glasses, and headwear.

## Contracts

### Configuration Schema (`rula.toml`)

```toml
[agents.bosmang]
accept_edits = true
args = []
auto_approve = true
context_docs = [".agents/personas/bosmang.md", "AGENTS.md"]
dna = "B01:E0:H06:BD02:A01"
effort = "high"
model = "gemini-3.8-flash"
role = "Lead Architecture & Systems Engineer"
runner = "agy"
system_prompt = "You are Bosmang..."
```

### Component Props (`src/components/ui/AppAgentPortrait.vue`)

- `dna?: string | null`
- `agentId?: string | null`
- `size?: number` (default `48`)

### Manifest (`public/portraits/manifest.json`)

- `version`, `tileSize: { width: 64, height: 96 }`, `layers: { base, expression, beard, hair, accessory }`.

## Implementation Status

| Item | Status | Evidence |
| :--- | :--- | :--- |
| `public/portraits/manifest.json` & sanitized 64×96 PNG assets | Implemented | `public/portraits/manifest.json`, `public/portraits/assets/` |
| Deterministic `generate_random_dna` | Implemented | `src-tauri/src/kopeng/dna.rs` |
| `<AppAgentPortrait />` Canvas compositor & UI integration | Implemented | `src/components/ui/AppAgentPortrait.vue`, `src/components/taskbar/AgentTaskbar.vue`, `src/components/dialogue/FloatingChatOverlay.vue`, `src/components/modals/AddPersonaModal.vue` |
| Interactive Avatar Customizer Modal (Phase 2) | Roadmap | Deferred layer-picker modal |

## Related Specs

- [MCP Dialogue Channel and Chat Overlay](03-mcp-dialogue-and-chat-overlay.md)
- [Configuration Schema and Cascade](../01-architecture/02-configuration-schema-and-cascade.md)
- [UI Component Library and Composition Recipes](../07-ui/02-ui-component-library-and-recipes.md)

Supersedes legacy: `21_agent_pixel_portrait_dna.md`
