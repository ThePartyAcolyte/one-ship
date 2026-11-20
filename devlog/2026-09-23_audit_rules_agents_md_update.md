# Development Log: Updating AGENTS.md with Audit Architectural Rules

**Date:** 2026-09-23  
**Topic:** Formalization of Architectural Patterns and Design Principles into `AGENTS.md` (Sections 9, 10, 11) Based on Comprehensive Codebase Audit Findings

---

## 1. Summary of Protocol Additions

Following the successful execution and remediation of all 28 audit findings across Critical, High, Medium, and Low phases, [`AGENTS.md`](../AGENTS.md) has been updated with three dedicated architectural standards sections:

### 1.1 Section 9: Backend Concurrency, Async Safety & Data Integrity Standards (Rust & Tauri)
- **Poison-Resistant Concurrency:** Strict ban on bare `.unwrap()` / `.expect()` on `RwLock` and `Mutex`. Mandatory use of poison-safe accessors (`read_safe()` / `write_safe()`).
- **Async Runtime Purity:** Strict ban on synchronous filesystem calls (`std::fs`) and blocking CPU routines inside async tasks. Mandatory `tokio::fs` or `tokio::task::spawn_blocking`.
- **Strongly-Typed Domain Errors (`thiserror`):** Prohibited raw `Result<T, String>` in backend domain logic. Mandatory strongly-typed `thiserror` enums.
- **Mandatory Transactional Batching:** Strict ban on un-batched N+1 iterative database writes. Mandatory SQL transactions for iterative multi-row writes.
- **Ergonomic Database Extension Traits:** Standardized usage of `DbResultExt` and `DuckResultExt`.
- **Complete IPC Command Registration Invariant:** Mandatory synchronized registration of `#[tauri::command]` in `lib.rs` and frontend wrappers in `tauriInvoke.ts`.
- **Comprehensive Public Rustdoc Coverage:** Mandatory `///` comments on all public structs, enums, traits, and functions.

### 1.2 Section 10: Frontend Architecture, Lifecycle & State Management Standards (Vue & TypeScript)
- **Mandatory Event Listener Teardown:** Mandatory capture and invocation of unlisten promises on component unmount and store teardown to prevent memory leaks.
- **Domain-Bounded Pinia Stores (Anti-God-Store):** Pinia stores must maintain single responsibility and bounded size ($\le 500$ lines), separating concerns into domain stores (`dialogueStore`, `agentSessionStore`, `todoTechDebtStore`).
- **Strict Zero `any` TypeScript Policy:** Strict ban on `any` across application code, Pinia state, IPC payloads, and component props.
- **Composable Bridge Layer for External Transports:** Decoupling UI components from raw transport protocols (`contentWindow.postMessage`, raw WebSockets) via typed composables.
- **Stable Virtual DOM Keys (`v-for`):** Strict ban on array indices (`:key="idx"`) for mutable or streaming collections; mandatory immutable unique IDs (`:key="item.id"`).

### 1.3 Section 11: Configuration-Driven Architecture & Dynamic Theming Standards
- **Dynamic Configuration Schema (Zero Magic Numbers):** Operational knobs, timeouts, intervals, and buffer limits must be registered in `get_all_plugin_schemas()` and exposed in settings.
- **Dynamic Semantic Theme Variables:** Strict ban on hardcoded Tailwind color palette classes for state feedback; mandatory usage of Omarchy theme CSS variables (`--status-ok-color`, `--status-warn-color`, `--status-error-color` and derived color-mix tokens).

---

## 2. Verification

- Updated [`AGENTS.md`](../AGENTS.md) lines 206–271.
- Both `cargo check` and `npm run build` continue to pass with 0 errors.
