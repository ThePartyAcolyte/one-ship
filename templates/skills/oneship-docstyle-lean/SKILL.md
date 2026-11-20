---
name: oneship-docstyle-lean
description: Lean ADR & Micro-Specs living documentation standard.
version: 0.2.5
---

# Lean & ADR Documentation Standard

The Lean archetype prioritizes rapid decision records and concise module summaries over exhaustive upfront specifications.

## 1. Directory Structure
```text
docs/
├── README.md                     # Lightweight system overview and entrypoints
└── adr/                          # Architectural Decision Records
      ├── 0001_initial_stack_selection.md
      └── ...
```

## 2. ADR Format Rules
- Context: Problem statement and constraints.
- Decision: Concrete technical path chosen.
- Consequences: Positive and negative architectural trade-offs.
