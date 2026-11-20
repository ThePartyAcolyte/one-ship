# Code Audit — ONE-SHIP
**Date:** 2026-09-23 | **Methodology:** 6 Pro subagents in parallel — 28 specs + 39 Rust files + 68 Vue/TS files

> This audit supersedes all previous audit documents (2026-09-03 series).
> Remediation should be executed in phase order: Phase 1 → 2 → 3 → 4.

---

## Sub-Reports (Remediation Order)

| File | Phase | Scope | Top Finding |
|------|-------|-------|-------------|
| [01 — Critical](audit_01_critical.md) | Phase 1 | Crash risks, memory leaks, data integrity | 44 `unwrap()` in supervisor.rs; 22+ listener leaks; N+1 writes |
| [02 — High](audit_02_high.md) | Phase 2 | Architecture violations, spec/impl gaps | God stores; blocking async; missing thiserror; 3 missing specs |
| [03 — Medium](audit_03_medium.md) | Phase 3 | Hardcoded values, duplication, logging gaps | ~20 settings candidates; duplicated IO readers; opaque db.rs |
| [04 — Low](audit_04_low.md) | Phase 4 | Tiger Style, documentation, dead code | Unit suffixes; missing doc comments; i18n completeness |
| [Agent Isolation](audit_agent_isolation.md) | Phase 1 | Cross-project agent scoping & identity | Bare agent_id keys; suffix fallback hijacking; trapped delegations |
| [Conversation & Context Continuity](audit_conversation_context_continuity.md) | Design | Conversation lifecycle, runner-neutral context, restoration (Spec 30) | Runner switch archives the conversation; conversation runner outranks persona |

---

## Severity Legend

| Symbol | Level | Meaning |
|--------|-------|---------|
| 🔴 CRITICAL | Phase 1 | Crash risk, data corruption, or memory leak |
| 🟠 HIGH | Phase 2 | AGENTS.md violation, significant anti-pattern, spec contradiction |
| 🟡 MEDIUM | Phase 3 | Technical debt: duplication, missing logging, hardcoded values |
| 🔵 LOW | Phase 4 | Tiger Style, naming, documentation, dead code |

---

## Positive References (Do Not Change)

These modules are the best-written in the codebase and serve as implementation models:

- **`stats.rs`** — Tiger Style correctly applied (`used_percent`, `remaining_percent`, `reset_epoch_seconds`).
- **`accounts.rs`** — Logging done right: INFO on switches, keyring operations.
- **`mcp.rs`** — 17+ logs with `agent_id` context. Good per-agent traceability.
- **`visual_builder.rs`** — `DEV_SERVER_BOOT_TIMEOUT_MS` applies Tiger Style correctly.
- **`quotaUtils.ts`** — Quota logic correctly centralized; 4 AppQuota* components share math without duplication.
- **Spec 27 vs `ProjectVisualBuilderTab.vue`** — All Tauri commands exist in `visual_builder.rs`. No frontend-backend disconnects.
- **Multi-account (`accounts.rs`)** — AES-256-GCM implemented; `switch_agent_account_cmd` correctly resets active agents.

---

## Executive Summary

```
PHASE 1 — CRITICAL (5 items)
  supervisor.rs: 44 unwrap() on RwLock → crash risk on lock poison
  coreStore/dialogueStore: 22+ Tauri listeners with no unlisten() → memory leak
  db.rs:135: N+1 writes inside loop without SQL transaction
  supervisor.rs/db.rs/telemetry.rs: catastrophically low log coverage
  TypeScript: pervasive `any` type nullifies the type system

PHASE 2 — HIGH (8 items)
  kopeng/errors.rs: no typed error enums, all Result<T, String>
  dialogueStore.ts (~1500 lines): 6 unrelated domains in one store
  initializer.rs / flag_resolver.rs / runner.rs: blocking I/O in async
  Specs 03 (Tray Hub) and 02 (Error Curator): no implementation
  Spec 06 (Plugin trait): contradicts monolithic lib.rs reality
  Spec 20: switch_conversation_topic MCP tool missing
  Tauri invoke error handling: copy-pasted in 30+ places
  AppInput/AppTooltip violations in 6+ modals

PHASE 3 — MEDIUM (7 items)
  ~20 hardcoded values not exposed in SettingsModal
  supervisor.rs: stdout/stderr reader loop duplicated 3x
  initializer.rs + flag_resolver.rs: config resolution duplicated
  db.rs: error handling boilerplate repeated hundreds of times
  migration.rs: only 1 aggregate log instead of per-step
  ProjectVisualBuilderTab.vue: contentWindow.postMessage inline
  runner::run_project_task: #[tauri::command] but not registered

PHASE 4 — LOW (7 items)
  Tiger Style: ~8 variables missing _ms/_bytes/_count/_percent suffixes
  Cargo.toml: toml = "1.1.4" suspicious version
  Dead code: resolve_effective_bool_with_default, spawn_dbus_monitor
  v-for :key="idx" on dynamic devLogs list
  Hardcoded color tokens bypassing design system in VisualBuilder
  i18n: most modal strings hardcoded in English
  Doc comments: ProcessStatus, AgentState, ProcessSupervisor undocumented
```
