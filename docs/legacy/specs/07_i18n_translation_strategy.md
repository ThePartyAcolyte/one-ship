> [!WARNING]
> **LEGACY SPECIFICATION:** Superseded by [07-ui/06-localization-and-belta-language.md](../../specs/07-ui/06-localization-and-belta-language.md). Preserved for historical provenance only.

# Spec 07: i18n & Language Strategy

> [!NOTE]
> **Status: DEFERRED UNTIL PACKAGING (Post-Completion Translation Pass)**
> Active development of ONE-SHIP's codebase, UI, and documentation is conducted strictly in English (`AGENTS.md` §1.2). This 3-tier Belta localization strategy is parked until the application reaches functional completion and packaging, at which point translations will be applied as a dedicated, separate pass.

## 1. Overview and Core Philosophy
To maintain velocity, ONE-SHIP is developed **entirely in English**. Feature development is never slowed down to debate or write translations. Once the application is packaged, a dedicated translation pass will populate and refine `src/i18n.ts` following this specification and Spec 14.

## 2. The 3-Tier Language System
The user interface supports three distinct language modes, cycled by clicking the main application title:

1. `en` (English): The standard, pure English interface.
2. `belta_translated`: A learning tier. The primary labels are in Lang Belta, but there is a small, subtle subtitle beneath it containing the **literal English translation of the expression**, NOT the technical UI function.
   - *Example:* Instead of `Active Services`, the Belta term might be `Kapawulowda`, and the subtitle should read `Ships / Fleet` (the literal translation of Kapawulowda), to help the user learn the dialect contextually.
3. `belta_pure`: The fluent tier. Only the Lang Belta terms are shown, with no English subtitles.

## 3. Implementation Guidelines for the Language Expert Agent
- Do not use intrusive tooltips for translations. Tooltips interfere with navigation. Use the 3-tier subtitle system (in Vue, using `v-if="locale === 'belta_translated'"` for the subtitles).
- The `src/i18n.ts` file acts as the dictionary. Ensure the structure maps cleanly so Vue components can easily extract both the term and (if necessary) the literal translation subtitle.
- Code structures, directories, and variables can playfully use Belta terminology (e.g., `kopeng` for plugins, `bosmang` for orchestrator) but **user-facing configuration files like `rula.toml` must remain in standard English** (e.g., `[plugins.orchestrator]`) for clarity and universality.
