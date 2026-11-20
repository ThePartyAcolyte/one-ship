---
name: oneship-docstyle-sdd
description: Living Specifications (Bidirectional Spec-Driven) documentation standard.
version: 0.2.6
---

# Living Specifications (Bidirectional Spec-Driven) Standard

The Living Specifications standard establishes a bi-directionally synchronized specification layout under `docs/`. Specifications guide initial code implementation, but when architecture, technical needs, or code reality evolve, the specifications MUST be updated in the same turn.

## 1. Directory Structure
```text
docs/
├── 00_manifesto.md               # Ontological baseline rules, non-negotiable boundaries & agent alignment
├── 01_ubiquitous_language.md     # Domain dictionary, glossary, and core terminology
└── specs/                        # Sequenced technical specifications (living source of truth)
      ├── 01_system_architecture.md
      ├── 02_data_models_and_schemas.md
      └── ...
```

## 2. Document Format Rules
- Every specification must include YAML or Header metadata:
  - Document Title & Version
  - Domain & Subsystems
  - Maintainer & Invariants
- **Bidirectional Invariant:** Code is guided by specifications, and specifications are the materialization of system truth. When implementations or requirements change, specs must be updated in the same turn.

