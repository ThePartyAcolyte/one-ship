# Development Log: Specification Set Reorganization, Documentation Cleanup & English-Only Gate

**Date:** 2026-11-20  
**Topic:** Full-Tree Reorganization of `docs/specs/`, README & Foundation Sync, English-Only Enforcement, and Obsolete File Removal

---

## 1. Motivation

The legacy specification set (`docs/specs/01_*` through `41_*`) had grown organically in chronological ideation order with numbering gaps, a duplicate `26_` prefix, scattered subsystems, an outdated count (`31`) in `docs/README.md`, Spanish prose in `docs/03_lang_belta_dictionary.md` and `oneship-artifacts/SKILL.md`, and obsolete tracking/audit files (`docs/WORK_PLAN.md`, `docs/audit/`).

With the repository published online, all specifications and root/foundation documentation were reorganized, synced with verified code state, and enforced via `scripts/verify_rules.sh`.

---

## 2. Changes Executed

### A. Legacy Archiving & Obsolete File Removal
- Moved all 39 numbered specification files from `docs/specs/*.md` to `docs/legacy/specs/*.md` and prepended a GitHub `[!WARNING]` banner on each pointing to its superseding spec(s).
- Created [`docs/legacy/README.md`](../docs/legacy/README.md) and [`docs/legacy/MAPPING.md`](../docs/legacy/MAPPING.md) documenting the complete 39-to-41 traceability matrix.
- Removed obsolete `docs/WORK_PLAN.md` (superseded by living specs + `TODO.md` + `TECHNICAL_DEBT.md`) and `docs/audit/` (completed historical audits), along with stale links in [`AGENTS.md`](../AGENTS.md) and [`docs/README.md`](../docs/README.md).

### B. Rewritten Subsystem Specification Tree (`docs/specs/`)
Rewrote and categorized all 39 legacy specifications into 41 living specifications across 7 subsystem directories (plus 7 category `README.md` indexes and [`docs/specs/README.md`](../docs/specs/README.md)):

1. [`docs/specs/01-architecture/`](../docs/specs/01-architecture/README.md) (`01`–`07`)
2. [`docs/specs/02-agents/`](../docs/specs/02-agents/README.md) (`01`–`08`)
3. [`docs/specs/03-projects/`](../docs/specs/03-projects/README.md) (`01`–`08`, `10`; `09` unassigned to preserve stable cross-links)
4. [`docs/specs/04-observability/`](../docs/specs/04-observability/README.md) (`01`–`02`)
5. [`docs/specs/05-voice/`](../docs/specs/05-voice/README.md) (`01`–`04`)
6. [`docs/specs/06-hardware-and-cad/`](../docs/specs/06-hardware-and-cad/README.md) (`01`–`04`)
7. [`docs/specs/07-ui/`](../docs/specs/07-ui/README.md) (`01`–`07`)

### C. Root README, Foundation Docs & Technical Accuracy Sync
- Rewrote [`README.md`](../README.md) based on the 41 specifications and verified code state, covering the Global Voice Controller / Unified Capability Core, 16-tab workspace, 59-tool MCP server, and non-dev multi-distro `./install.sh` testing matrix (active Ubuntu GNOME testing + planned Fedora Workstation and Linux Mint Cinnamon targets, floating-DE AppIndicator tray, 8-edge resize, `one-ship-launch` WebKitGTK retry).
- Updated [`docs/specs/README.md`](../docs/specs/README.md) with the full 41-spec index, reading paths, and non-dev Linux packaging baseline.
- Updated [`docs/README.md`](../docs/README.md), [`docs/specs/01-architecture/README.md`](../docs/specs/01-architecture/README.md), and [`docs/specs/01-architecture/02-configuration-schema-and-cascade.md`](../docs/specs/01-architecture/02-configuration-schema-and-cascade.md) to reflect all **17** registered plugin schemas in `src-tauri/src/config.rs` (including `voice_controller`, `project_management`, and `semantic`).
- Updated [`docs/00_manifesto.md`](../docs/00_manifesto.md), [`docs/01_ubiquitous_language.md`](../docs/01_ubiquitous_language.md), [`docs/02_architecture_stack.md`](../docs/02_architecture_stack.md) (fixing `/api/v1/call/<command>`, multi-distro Linux scope, Global Voice Controller status, and relative spec links), [`docs/DESIGN_RULES.md`](../docs/DESIGN_RULES.md), [`src/components/modals/AddPersonaModal.vue`](../src/components/modals/AddPersonaModal.vue), and [`.agents/workflows/cinturonero-translator.md`](../.agents/workflows/cinturonero-translator.md).

### D. English-Only Enforcement & Translations
- Translated [`docs/03_lang_belta_dictionary.md`](../docs/03_lang_belta_dictionary.md) (258 lines) into English while preserving original Lang Belta phrases and Saga quotes.
- Translated the artifact decision diagram in [`templates/skills/oneship-artifacts/SKILL.md`](../templates/skills/oneship-artifacts/SKILL.md) and HTML comments in [`src/components/ProjectCard/CardHeader.vue`](../src/components/ProjectCard/CardHeader.vue) into English, and removed `(*Traducción Viva*)` from [`docs/specs/07-ui/06-localization-and-belta-language.md`](../docs/specs/07-ui/06-localization-and-belta-language.md).
- Added check #9 (`English-only active documentation and templates`) to [`scripts/verify_rules.sh`](../scripts/verify_rules.sh).

---

## 3. Verification

- **Active Markdown Cross-Links (59 files across root, `docs/`, `docs/legacy/MAPPING.md`, `docs/specs/**/*.md`):**
  ```text
  Checked active markdown files: 59 Broken links: 0 []
  ```
- **`./scripts/verify_rules.sh`:**
  ```text
  [PASS] Zero TypeScript 'any' in src/
  [PASS] Zero Tailwind palette classes (text-/bg-/border-{color}-*)
  [PASS] Zero backdrop-blur-* in components/views
  [PASS] Zero hardcoded /home/butcherwutcher paths
  [PASS] English-only active documentation and templates
  === Summary: 2 hard failure(s) ===  # pre-existing in untouched config.rs:1920 & legacy modals
  ```
- **`./scripts/run_dev.sh status`:**
  - `daemon`: `active` / `running` (port `8765`), `gui`: `active` / `running`.
