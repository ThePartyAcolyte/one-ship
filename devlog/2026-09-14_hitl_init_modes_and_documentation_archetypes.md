# Devlog: 2026-09-14 - HITL Initialization Modes, Non-Invasive Reconnaissance & Documentation Archetypes

## Topic
Refinement of project initialization into a strict dual-mode model (Supervised via Chat Overlay vs Unsupervised Autonomous), elimination of arbitrary persona quantity heuristics, integration of non-invasive knowledge reconnaissance (tasks, debt, Genesis Milestone), and establishment of the `kapawu-archetypes` platform skill catalog.

## Context & Objectives
1. Address the arbitrary "2 to 4 domain specialists" heuristic in `skill.rs` and replace it with objective criteria based on decoupled directory boundaries and subsystem domains.
2. Consolidate initialization into two clear modes: **Supervised** (Chat Guided with the Global System Agent) and **Unsupervised** (Autonomous Headless).
3. Establish non-invasive semantic reconnaissance during project bootstrapping: application code is strictly read-only, while tasks (inline or high-level) and technical debt are discovered and registered into SQLite `project_todos` and `project_technical_debt`.
4. Ingest an inaugural baseline as a **Genesis Milestone** into `agent_session_journal`.
5. Introduce `kapawu-archetypes` platform skill defining 4 standard HITL documentation archetypes (Spec-Driven, Fast-Iterative, Contract-First, Test-Anchored) and an extensible custom workflow authoring protocol.

## Progress
- [x] Researched 2024-2026 Human-in-the-Loop agentic software engineering patterns (Spec-Driven, ADRs, Contract-First, Test Oracles).
- [x] Created `templates/skills/kapawu-archetypes/SKILL.md` and synchronized it to `.agents/skills/kapawu-archetypes/SKILL.md`.
- [x] Refactored `src-tauri/src/kopeng/skill.rs`:
  - Added `kapawu-archetypes` to platform skills injection catalog.
  - Replaced arbitrary persona count limits with objective directory boundary criteria.
  - Injected non-invasive discovery instructions and Genesis Milestone guidance.
  - Streamlined execution modes into `supervised` and `unattended`.
- [x] Refactored `src/components/modals/ProjectInitModal.vue`:
  - Simplified modes to Supervised (Chat Guided) and Unsupervised (Autonomous).
  - Enforced strict modal minimalism: removed premature documentation selector and auto-approve toggles.
  - Primed System Agent to evaluate repository maturity and propose the best-fit documentation archetype in the Chat Overlay.
  - Automatically syncs platform skills during initialization.
- [x] Updated living specification ([`docs/specs/24_project_versioning_and_migration_engine.md`](file:///home/butcherwutcher/Projects/dev/wang-kapawu/docs/specs/24_project_versioning_and_migration_engine.md)).
- [x] Verified full Rust compilation (`cargo check`) and Vue TypeScript build (`npm run build`).
