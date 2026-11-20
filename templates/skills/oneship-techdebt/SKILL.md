---
name: oneship-techdebt
description: Technical debt classification, severity triage, and database recording protocol.
version: 0.2.5
---

# ONE-SHIP Technical Debt Protocol

This skill establishes the rules for identifying, classifying, and recording technical debt into the project's Knowledge Graph.

---

## 1. Zero Silent Debt Policy

In ONE-SHIP projects:
- Never accumulate hidden shortcuts, unhandled edge cases, or temporary workarounds.
- Whenever an architectural compromise is made or a structural bottleneck is discovered during development, it MUST be recorded immediately via the MCP tool `record_technical_debt`.

---

## 2. Severity Classification Matrix

| Severity | Criteria & Impact | Example Scenarios |
| :--- | :--- | :--- |
| **`critical`** | Potential data loss, security exposure, state corruption, or fatal panic/crash. | Unbounded memory leak, race condition in file writes, unvalidated destructive IPC commands. |
| **`high`** | Major performance degradation, scalability blocker, or high-risk architectural coupling. | Synchronous blocking I/O on UI/event loops, missing database indexes on growing tables. |
| **`medium`** | Code duplication, missing automated tests on key paths, or suboptimal error diagnostics. | Repeated logic across modules, fragile regex parsing instead of proper AST/tokenizer. |
| **`low`** | Minor stylistic inconsistencies, non-blocking cleanup, or aesthetic debt. | Deprecated variable names, non-critical comments cleanup. |

---

## 3. When to Record Technical Debt (`record_technical_debt`)

Invoke `record_technical_debt` whenever:
1. **Pragmatic Shortcut Taken:** You implement a temporary workaround to unblock an immediate milestone.
2. **Structural Bottleneck Discovered:** You uncover deep legacy architecture that requires a dedicated refactoring phase.
3. **Missing Test or Validation Gap:** You observe critical execution paths lacking automated verification.

> **Actionable Remediation Required:** Every recorded debt item must include a concrete `remediation_plan` outlining the recommended architectural solution for future turns.
