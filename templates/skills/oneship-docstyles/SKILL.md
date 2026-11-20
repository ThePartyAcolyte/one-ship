---
name: oneship-docstyles
description: Catalog and operational protocol for scaffolding project documentation styles (Spec-Driven, Lean Task-Driven, Contract-First, Test-Anchored) upon resolving initialization flags.
version: "0.2.5"
---

# ONE-SHIP Docstyles & Agentic Steering Catalog

This skill provides operational rules, directory layouts, and governance models for scaffolding and maintaining project documentation tailored for solo developers and small teams with strict Human-in-the-Loop (HITL) control.

---

## 1. Core Principles

1. **On-Demand Scaffolding (Phase 2 Invariant):**
   - During Phase 1 initialization, documentation is NOT eagerly or presumptuously created. The agent raises an initialization flag (`flag_type = 'docs'`).
   - The documentation hierarchy is scaffolded exclusively in Phase 2 when the developer clicks `[! INIT: DOCS]` and chooses their preferred Docstyle.
2. **Context Anchoring Over Context Stuffing:**
   - LLMs suffer attention degradation when forced to navigate sprawling raw codebases without boundaries.
   - Documentation acts as machine-readable steering rails and persistent memory, keeping agent attention focused on validated invariants.
3. **No Arbitrary Persona Limits:**
   - Configure agent specialist personas based strictly on actual decoupled directory boundaries and subsystem domains (e.g. a single lead for compact scripts; core, frontend, and git only when distinct subsystems exist). Never enforce arbitrary minimums or maximums.

---

## 2. The Four Standard Docstyles

### Docstyle 1: Living Specifications (Bidirectional Spec-Driven)
*Recommended for complex platforms, desktop systems, domain-driven architectures, and long-term multi-subsystem projects.*

- **Philosophy:** *"Specifications guide development, but documentation is a living, bi-directional process. When technical reality and implementation evolve, specifications must be updated in the same turn."*
- **Directory Layout:**
  ```text
  .
  ├── AGENTS.md                   # Universal governance & bounded specialist roles
  ├── TODO.md                     # Active atomic task backlog (pending items only)
  ├── docs/
  │   ├── 00_manifesto.md         # Ontological baseline rules, non-negotiable boundaries & agent alignment
  │   ├── 01_ubiquitous_language.md # Domain lexicon & ubiquitous glossary (prevents semantic drift)
  │   ├── adr/                    # Architecture Decision Records (immutable negative constraints)
  │   │   └── 0001_initial_architecture.md
  │   └── specs/                  # Numbered living specifications (synced bi-directionally with code)
  │       ├── 01_core_subsystem.md
  │       └── 02_ui_components.md
  ```
- **Lifecycle & Governance:**
  - Initial scaffolding captures ontological rules (`00_manifesto.md`), domain glossary (`01_ubiquitous_language.md`), and foundational specs (`docs/specs/*.md`).
  - Developers and agents are guided by specifications rather than constrained by rigid waterfall paperwork.
  - When design decisions, technical constraints, or code implementations evolve, agents **MUST update `docs/specs/*.md` bi-directionally in the same turn** to maintain synchronization with reality.


### Docstyle 2: Fast-Iterative (Lean Task-Driven)
*Recommended for solo developers, MVPs, proof-of-concepts, command-line utilities, and rapid prototyping.*

- **Philosophy:** *"Speed to working software with lightweight operational guardrails."*
- **Directory Layout:**
  ```text
  .
  ├── AGENTS.md                   # Operational cheatsheet (<80 lines: build/test/run, style)
  ├── ROADMAP.md                  # High-level vision, phase milestones, and release scope
  └── TODO.md                     # Active task list with checkboxes and acceptance criteria
  ```
- **Lifecycle & Governance:**
  - Tasks in `TODO.md` contain inline acceptance criteria.
  - Agents execute directly against task items, verify with compiler/linter commands, and mark tasks complete.

### Docstyle 3: Contract & API-First
*Recommended for web services, REST/gRPC/GraphQL APIs, microservices, and decoupled frontend/backend stacks.*

- **Philosophy:** *"Interface contracts are the single source of truth. Mock early, implement and verify against schemas."*
- **Directory Layout:**
  ```text
  .
  ├── AGENTS.md                   # Governance & schema validation constraints
  ├── schemas/                    # Machine-readable schemas (OpenAPI, Protobuf, JSONSchema)
  │   └── api_v1.yaml
  ├── docs/
  │   └── rfc/                    # Request for Comments for breaking changes
  │       └── 001_api_baseline.md
  └── tests/contract/             # Automated schema compliance test suite
  ```
- **Lifecycle & Governance:**
  - Schemas in `schemas/` cannot be modified without human sign-off via RFC.
  - Frontend and backend agents operate independently against generated types and mocks.

### Docstyle 4: Test-Anchored / High-Reliability
*Recommended for financial calculation engines, cryptographic modules, compilers/parsers, and safety-critical algorithms.*

- **Philosophy:** *"Agent code is unverified until proven correct by deterministic, non-agent-modifiable test suites."*
- **Directory Layout:**
  ```text
  .
  ├── AGENTS.md                   # Strict execution rules: test suites are immutable to agents
  ├── specs/invariants.md         # Mathematical properties, edge cases, and safety bounds
  ├── tests/
  │   ├── property/               # Property-based fuzzing harnesses (e.g. proptest, fast-check)
  │   └── fixtures/               # Golden files and verified input-output vectors
  └── docs/adr/                   # Algorithm and security decision records
  ```
- **Lifecycle & Governance:**
  - Invariants are mathematically stated in `specs/invariants.md`.
  - Agents execute in a Red-to-Green cycle against existing tests, forbidden from altering test assertions.

---

## 3. Custom Docstyle Authoring Protocol

When a developer desires a bespoke workflow:
1. The developer interacts in the Chat Overlay.
2. The agent queries the developer regarding:
   - Target review cadence (per-file vs. per-feature).
   - Preferred document formats (Markdown, OpenAPI, AsciiDoc).
   - Quality gates (linters, typecheckers, custom test scripts).
3. The agent compiles the custom methodology into a new skill file (`.agents/skills/custom-<name>/SKILL.md`) and updates `AGENTS.md` to reference it.
4. The custom skill can be exported to `~/.config/one-ship/skills/` for reuse across projects.

---

## 4. Resolving the `docs` Initialization Flag

When resolving the `docs` initialization flag in Phase 2:
1. The developer selects their desired Docstyle from the options presented in the Chat Overlay.
2. The agent scaffolds the corresponding directory structure and template files.
3. The agent invokes the MCP tool `resolve_initialization_flag`:
   ```json
   {
     "flag_id": 1,
     "status": "resolved"
   }
   ```
4. The agent records the documentation milestone via `record_session_minutes`.
