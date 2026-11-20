# Localization and Belta Language Strategy

> [!NOTE]
> **Status: DEFERRED UNTIL PACKAGING (Post-Completion Translation Pass)**
> Active development of ONE-SHIP's codebase, UI, and documentation is conducted strictly in English. This 3-tier Lang Belta localization strategy and the Cinturonero translation protocol are parked until the application reaches functional completion and packaging, at which point localization will be executed as a dedicated, standalone pass.

## Purpose

Defines ONE-SHIP's English-first development policy, the optional 3-tier Lang Belta immersion mode (`en` -> `belta_translated` -> `belta_pure`), and the Cinturonero Translator protocol that governs which UI strings are translated into Belter Creole and how literal learning glosses are constructed without slowing down feature development or obscuring developer workflows.

## Scope

In scope: the 3-tier language system in `src/i18n.ts`, translation boundaries between domain identity/actions and functional explanations/diagnostics, the linguistic corpus and CLI search workflow, grammatical aspect markers, and the step-by-step review protocol for translating UI text.

Out of scope: standard English UI copy and unit formatting rules ([Design System Foundations](01-design-system-foundations.md)) and configuration file schemas ([Configuration Schema and Cascade](../01-architecture/02-configuration-schema-and-cascade.md)).

## Model / Design

### Core Philosophy: English-First Velocity and Da Doctrina de ONE-SHIP

To maintain development velocity, ONE-SHIP is built **entirely in English**. Feature development is never slowed down to debate or write translations. Once the application is functionally complete and packaged, a dedicated translation pass populates and refines `src/i18n.ts`.

When active, the **Cinturonero Translator** acts as a cultural and linguistic interpreter of the Belter (*Beltalowda*) worldview rather than a mechanical lookup table:

- **Survival in the Vacuum:** Direct, functional, unambiguous, with dark humor and fierce pragmatism.
- **The One Ship Doctrine (*ONE-SHIP*):** The developer workstation, the background daemon, and the agent crew are a single vessel. A crashed service is a hull breach; a degraded service is life support strain.
- **Living Translation:** Rather than pre-locking an exhaustive, rigid dictionary upfront, translations are constructed organically as new UI elements, buttons, workflows, and subsystems emerge, guided by the full linguistic corpus and grammatical rules.

### The Golden Rule: Translation Boundaries

Strict separation is enforced between **Identity/Action** (eligible for Lang Belta) and **Explanation/Diagnostics/Configuration** (strictly preserved in English):

```text
+-------------------------------------------------------------------------+
|                  CINTURONERO TRANSLATION BOUNDARY MATRIX                |
+------------------------------------+------------------------------------+
|   TRANSLATE TO CINTURONERO (Belta) |     STRICTLY PRESERVE IN ENGLISH   |
+------------------------------------+------------------------------------+
| 1. System Entities & Subsystems    | 1. Functional Explanations         |
|    - Main Hub -> ONE-SHIP          |    - What a tool does              |
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

- Code structures, directories, and variables may playfully use Belta terminology (e.g. `kopeng` for plugins, `bosmang` for orchestrator), **but user-facing configuration files like `rula.toml` must remain in standard English** (e.g. `[plugins.orchestrator]`) for clarity and universality.

### The 3-Tier Localization Model (`src/i18n.ts`)

The user interface supports three distinct language modes, cycled by clicking the main application title (`ONE-SHIP`):

1. **Tier 1 — `en` (Standard Developer English):** Clear, universal, concise English for standard cockpit operation (e.g. `Launch`, `Active Services`, `All nominal`).
2. **Tier 2 — `belta_translated` (The Learning Tier):** The primary label is rendered in **Lang Belta**, paired with a small, subtle subtitle containing the **literal English translation of the Belta expression**—NOT the technical UI function it replaced.
   - *Rationale:* Teaches the developer the actual dialect and worldview contextually rather than duplicating the button label.
   - *Examples:*
     - Button: `Gowork` | Subtitle: `(Go to work)` — *[Literal meaning of Gowork, NOT `'Launch'`]*
     - Header: `KAPAWULOWDA` | Subtitle: `(Ships / Fleet)` — *[Literal meaning, NOT `'Active Services'`]*
     - Status: `Kowlting gonya gut` | Subtitle: `(Everything is gonna be good)` — *[Literal meaning, NOT `'All nominal'`]*
     - Prompt: `Sasa ke?` | Subtitle: `(Do you know?)` — *[Literal meaning, NOT `'Confirm'`]*
3. **Tier 3 — `belta_pure` (The Fluent / Hardcore Tier):** Pure Belter Creole terms with no English subtitles or hints, for experienced users navigating by muscle memory and cultural immersion (e.g. `Gowork`, `KAPAWULOWDA`, `Kowlting gonya gut`, `Sasa ke?`).

#### Rendering Guidelines

- Do not use intrusive hover tooltips for translations; tooltips interfere with navigation. Use the 3-tier subtitle system (in Vue templates, `v-if="locale === 'belta_translated'"`).
- `src/i18n.ts` acts as the structured dictionary so Vue components can cleanly extract both the term and, when in Tier 2, the literal translation subtitle.

### Linguistic Toolkit and Grammatical Markers

Translations draw from the compiled corpus in [docs/tekidok_lang_belta.json](../../../docs/tekidok_lang_belta.json) (725 words, 262 idioms, 70 sentences, 26 numbers, 11 grammar rules, 39 cultural notes) queried via [scripts/belta_search.py](../../../scripts/belta_search.py):

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

#### Grammatical Markers

- **Ongoing aspect (`ando`):** Active background processes or running services (e.g. `ando du work` — currently executing).
- **Habitual aspect (`tili`):** Daemon watch loops and recurring telemetry pollers.
- **Perfective aspect (`finyish`):** Finished builds, cleaned caches, completed Git operations.
- **Inchoative / prospective (`gonya`):** Pending actions, scheduled tasks, starting phases (`gonya gowork`).
- **Plural collective (`-lowda`):** Exclusively on pronouns/collectives (e.g. `kapawulowda` for the fleet of services; `kopenglowda` for the agent crew).
- **Interrogative particle (`ke`):** Placed at the end of prompt confirmations (`Sasa ke?` — Do you know / understand?).

### Step-by-Step Review Protocol for New UI Text

Whenever a new UI view, component, or specification enters the post-packaging translation pass:

1. **Extract strings:** Identify every raw string in the template or component.
2. **Apply boundary filter:** Leave descriptions, instructional hints, and low-level diagnostics in plain English; flag domain nouns, action verbs, health statuses, and atmospheric quotes for Cinturonero translation.
3. **Linguistic query:** Run `scripts/belta_search.py` to inspect root words, related idioms, and existing sentence patterns.
4. **Determine literal gloss:** Construct the literal English meaning for Tier 2 (`belta_translated`).
5. **Update `src/i18n.ts`:** Register the key under `en`, `belta_translated`, and `belta_pure`.
6. **Verify UI rendering:** Confirm that Tier 2 renders the subtle subtitle (`v-if="locale === 'belta_translated'"`) without intrusive hover tooltips.

## Contracts

- **i18n instance (`src/i18n.ts`):** `vue-i18n` (`legacy: false`, default `locale: 'en'`, `fallbackLocale: 'en'`), exporting `messages` keyed by `'en'`, `'belta_translated'`, and `'belta_pure'`.
- **Locale cycler (`src/App.vue`):** Clicking the main `ONE-SHIP` title cycles `en` -> `belta_translated` -> `belta_pure` -> `en`.
- **Linguistic corpus and CLI:** `docs/tekidok_lang_belta.json` and `scripts/belta_search.py` (`--grammar`, `--notes`).
- **Configuration boundary:** TOML keys in `rula.toml` and `config.toml` remain strictly English regardless of active UI locale.

## Implementation Status

Verified against the repository:

- `src/i18n.ts` (`createI18n` with `en`, `belta_translated`, and `belta_pure` seed dictionaries), `src/main.ts` (`app.use(i18n)`), and `src/App.vue` (title-click cycler across the three locales): **Implemented** (partial seed coverage).
- `docs/tekidok_lang_belta.json` and `scripts/belta_search.py`: **Implemented**.
- Full UI string coverage and Tier 2 subtitle rendering across all workspace panels: **Partial / Deferred** until post-packaging translation pass (active UI components remain strictly English).

## Related Specs

- [Design System Foundations](01-design-system-foundations.md)
- [UI Component Library and Composition Recipes](02-ui-component-library-and-recipes.md)
- [Configuration Schema and Cascade](../01-architecture/02-configuration-schema-and-cascade.md)

Supersedes legacy: `07_i18n_translation_strategy.md`, `14_cinturonero_translator_protocol.md`
