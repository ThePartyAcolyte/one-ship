---
description: Review UI specifications and components to translate titles, actions, and states into Lang Belta (Cinturonero) across the 3-tier i18n system.
---

# Cinturonero Translator Workflow

This workflow guides the Language Expert Agent in evaluating new UI texts, applying the Cinturonero translation boundaries, consulting the linguistic database, and generating 3-tier translations in `src/i18n.ts`.

## Prerequisites & Resources
- Reference Spec: `docs/specs/08_cinturonero_translator_protocol.md`
- Database: `docs/tekidok_lang_belta.json`
- Search CLI: `scripts/belta_search.py`
- Localization File: `src/i18n.ts`

---

## Steps

1. **Scan UI Changes for Candidate Strings**
   Inspect newly created or modified UI templates (`src/**/*.vue`), specs (`docs/specs/`), or task requirements to list all user-facing strings.

2. **Classify Strings Against the Golden Rule**
   Separate candidates into:
   - **Translate to Belta:** Core entities, main buttons, health status badges, and cockpit quotes.
   - **Keep in English:** Tool explanations, user instructions, stack traces, diagnostics, and config keys.

3. **Query the Linguistic Database**
   For each string to be translated, run the search script to discover authentic Belter roots, idioms, or sentences:
   // turbo
   ```bash
   python3 scripts/belta_search.py "<concept_or_keyword>"
   ```

4. **Synthesize the 3-Tier Translations**
   Construct the 3 tiers:
   - `en`: Standard English UI term.
   - `belta_translated`: Lang Belta term + literal English meaning of the Belta expression in subtitle/gloss (NOT the technical UI function).
   - `belta_pure`: Lang Belta term only.

5. **Update `src/i18n.ts`**
   Add the new entries to the appropriate message groups (`hub`, `status`, `action`, `prompt`, `error`, `metrics`, etc.) maintaining identical structure across `en`, `belta_translated`, and `belta_pure`.

6. **Verify Component Binding**
   Ensure Vue components render the subtitle conditionally for `belta_translated` using `v-if="locale === 'belta_translated'"` rather than intrusive hover tooltips.
