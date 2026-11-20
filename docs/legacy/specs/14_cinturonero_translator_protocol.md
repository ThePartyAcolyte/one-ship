> [!WARNING]
> **LEGACY SPECIFICATION:** Superseded by [07-ui/06-localization-and-belta-language.md](../../specs/07-ui/06-localization-and-belta-language.md). Preserved for historical provenance only.

# Spec 14: Cinturonero Translator Protocol & Living Translation Architecture
**Status:** Parked until Packaging (Post-Completion Translation Pass)  
**Role:** Language & Belt Creolization Specialist (Cinturonero Translator)  
**Corpus:** `docs/tekidok_lang_belta.json` (725 words, 262 idioms, 70 sentences, 26 numbers, 11 grammar rules, 39 cultural notes)

> [!NOTE]
> **Status: DEFERRED UNTIL PACKAGING**
> Per `AGENTS.md` §1.2 and `docs/README.md`, all active code, UI components, and specifications are written strictly in English during development. The Cinturonero translation protocol defined below is parked until ONE-SHIP is functionally complete and packaged, after which localization will be executed as a standalone process.

---

## 1. Role Purpose and Philosophy: Da Doctrina de ONE-SHIP

The **Cinturonero Translator** is not a static dictionary lookup or a fixed list enforcer. It is an active linguistic and cultural interpreter representing the Belter (*Beltalowda*) worldview:
- **Survival in the Vacuum:** Direct, functional, unambiguous, with dark humor and fierce pragmatism.
- **The One Ship Doctrine (*ONE-SHIP*):** The developer workstation, the background daemon, and the agent crew are a single vessel. A crashed service is a hull breach; a degraded service is life support strain.
- **Living Translation (*Traducción Viva*):** Rather than pre-locking an exhaustive, rigid DDD dictionary upfront, translations are constructed organically as new UI elements, buttons, workflows, and subsystems emerge in development. The Translator uses deep cultural knowledge, grammatical aspects, and the full linguistic database to make context-aware decisions.

---

## 2. The Golden Rule: Translation Boundaries

To keep the developer experience frictionless and clear, strict separation is enforced between **Identity/Action** and **Explanation**:

```
+-------------------------------------------------------------------------+
|                  CINTURONERO TRANSLATION BOUNDARY MATRIX                |
+------------------------------------+------------------------------------+
|   TRANSLATE TO CINTURONERO (Belta) |     STRICTLY PRESERVE IN ENGLISH   |
+------------------------------------+------------------------------------+
| 1. System Entities & Subsystems    | 1. Functional Explanations         |
|    - Main Hub -> ONE-SHIP       |    - What a tool does              |
|    - Daemon -> Bosmang             |    - Why a feature exists          |
|    - Services -> Kapawulowda       |    - Impact warnings               |
|    - Plugins/Skills -> Kopeng      |                                    |
|    - Config file -> Rula           | 2. Step-by-Step Instructions       |
|                                    |    - "Select a directory..."       |
| 2. Primary Actions & Buttons       |    - "Type project name..."        |
|    - Launch -> Gowork              |                                    |
|    - Stop/Kill -> Kwit             | 3. Diagnostics & Raw Data          |
|    - Restart -> Mo du              |    - stderr / stdout streams       |
|    - Inspect -> Mera               |    - Stack traces, panic lines     |
|    - Confirm -> Sasa ke?           |    - OS-level error codes          |
|    - Cancel -> Nating              |                                    |
|                                    | 4. Developer Configurations        |
| 3. Status Badges & Telemetry       |    - TOML keys ([plugins.git])     |
|    - Nominal -> Kowlting gonya gut |    - CLI flags (--force, --port)   |
|    - Working -> Mi du work, bosmang|    - Code symbol names             |
|    - Degraded -> Xianlang          |                                    |
|    - Crashed -> Pashangfong        |                                    |
|                                    |                                    |
| 4. Atmospheric Quotes & Empty State|                                    |
|    - Welcome & empty state lore    |                                    |
|    - Motivational crew aphorisms   |                                    |
+------------------------------------+------------------------------------+
```

---

## 3. The 3-Tier Localization Model (`src/i18n.ts`)

Every user-facing label subject to translation must provide all 3 tiers:

### Tier 1: `en` (Standard Developer English)
- Clear, universal, concise English for standard terminal/cockpit operation.
- Example: `Launch`, `Active Services`, `All nominal`.

### Tier 2: `belta_translated` (The Learning Tier)
- The primary label is rendered in **Lang Belta**.
- A subtle subtitle or secondary gloss contains the **literal English translation of the Belta expression**, NOT the technical UI label that was replaced.
- *Rationale:* Teaches the developer the actual dialect and worldview contextually, rather than simply duplicating the button function.
- *Examples:*
  - UI Button: `Gowork` | Subtitle: `(Go to work)` — *[Literal meaning of Gowork, NOT 'Launch']*
  - Section Header: `KAPAWULOWDA` | Subtitle: `(Ships / Fleet)` — *[Literal meaning, NOT 'Active Services']*
  - Status: `Kowlting gonya gut` | Subtitle: `(Everything is gonna be good)` — *[Literal meaning, NOT 'All nominal']*
  - Prompt: `Sasa ke?` | Subtitle: `(Do you know?)` — *[Literal meaning, NOT 'Confirm']*

### Tier 3: `belta_pure` (The Hardcore / Fluent Tier)
- Pure Belter Creole terms without English subtitles or hints.
- For experienced Belters who navigate by muscle memory and cultural immersion.
- *Examples:* `Gowork`, `KAPAWULOWDA`, `Kowlting gonya gut`, `Sasa ke?`.

---

## 4. Linguistic Toolkit & Query Workflow

The translator has access to the compiled database in [`docs/tekidok_lang_belta.json`](../tekidok_lang_belta.json) via the CLI search tool [`scripts/belta_search.py`](../../scripts/belta_search.py):

```bash
# Query any English or Belta concept
python3 scripts/belta_search.py "friend"
python3 scripts/belta_search.py "ship"
python3 scripts/belta_search.py "work"

# Review grammatical aspect and mood markers
python3 scripts/belta_search.py --grammar

# Review cultural notes and sociolinguistic context
python3 scripts/belta_search.py --notes
```

### Grammatical Markers to Leverage:
- **Ongoing Aspect (`ando`):** Use for active background processes or running services (e.g., `ando du work` - currently executing).
- **Habitual Aspect (`tili`):** Use for daemon watch loops, recurring telemetry pollers.
- **Perfective Aspect (`finyish`):** Use for finished builds, cleaned caches, completed git operations.
- **Inchoative / Prospective (`gonya`):** Use for pending actions, scheduled tasks, starting phases (`gonya gowork`).
- **Plural Collective (`-lowda`):** Used exclusively on pronouns/collectives (e.g. `kapawulowda` for the entire fleet of services; `kopenglowda` for the agent crew).
- **Interrogative particle (`ke`):** Placed at the end of prompt confirmations (`Sasa ke?` - Do you know / understand?).

---

## 5. Step-by-Step Review Protocol for New UI Texts

Whenever a new UI view, component, or specification is submitted:

1. **Extract Strings:** Identify every raw string in the template or component.
2. **Apply Boundary Filter:**
   - If it is a description, instructional hint, or low-level diagnostic: leave in plain English (in Tier 1 and fallback).
   - If it is a domain noun, action verb, health status, or atmospheric quote: flag for Cinturonero translation.
3. **Linguistic Query:** Use `scripts/belta_search.py` to inspect root words, related idioms, and existing sentence patterns.
4. **Determine Literal Gloss:** Construct the literal English meaning for Tier 2 (`belta_translated`).
5. **Update `src/i18n.ts`:** Register the key under `en`, `belta_translated`, and `belta_pure`.
6. **Verify UI Rendering:** Verify that in Vue templates, Tier 2 displays the subtle subtitle (`v-if="locale === 'belta_translated'"`), avoiding intrusive hover tooltips.
