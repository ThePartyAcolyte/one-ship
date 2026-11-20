# Devlog: Agent Launch Refactor & Persona Quality Pass

**Date:** 2026-09-04
**Scope:** Agent management pipeline, rula.toml agent configs, persona files

---

## Summary

Executed a three-part refactor of the agent launch and persona system, addressing
accumulated quality issues identified during a post-refactorization review.

---

## Changes Applied

### 1. context_docs: Reference Injection (herdr.rs)

**Problem:** `launch_herdr_agent` was reading and dumping the full content of every
file in `context_docs` into the first agent prompt. For agents like `bosmang` (7 files),
this created 10–20 KB initial prompts before the agent understood its task.

**Fix:** Replaced file-content dump with a bullet-point reference list. Agents receive
paths under a `[CONTEXT REFERENCE]` header and read files on demand. No `std::fs::read_to_string`
calls at launch time.

**File:** `src-tauri/src/kopeng/herdr.rs` — lines 635–672 (step 5 of `launch_herdr_agent`).

---

### 2. Model Selection: /model Injection (herdr.rs + rula.toml)

**Problem:** `args = ["--model", "claude-sonnet-4-6"]` was placed after `--` in the
`herdr agent start` call. Herdr's PTY bootstrap sequence does not reliably carry
`AGENT_ARG` values through to the interactive session, causing model selection to be silently
discarded.

**Fix:** Added a dedicated `model` field to the `[agents.<name>]` schema. If set, Kapawu
injects `/model <value>` as the first prompt to the agent pane, after the PTY is stable,
using `herdr agent prompt`. This uses agy's native in-session model switching which
survives the startup sequence.

**Files:**
- `src-tauri/src/kopeng/herdr.rs` — new step 5a in `launch_herdr_agent`.
- `rula.toml` — `[agents.inalowda]`: replaced `args = ["--model", "..."]` with `model = "claude-sonnet-4-6"`.

---

### 3. Persona Quality Rewrite (all 5 persona files)

**Problem:** Persona prompts were over-specified with codebase snapshots (component lists,
absolute file paths, specific command checklists). These froze design decisions in time,
broke on path changes, and reduced agent flexibility as the project evolves.

**Fix:** Rewrote all 5 persona files (`bosmang`, `frontend`, `mcp`, `inalowda`, `lang_belta`)
following a principled structure:
- Identity & Mission (stable)
- Domain Ownership (conceptual, not file-enumerated)
- Decision Protocols (how to reason, not what to decide)
- Primary Reference Documents (where to look, not the content itself)
- Operational Checklist (principles, not toolchain snapshots)

**Additional fixes in persona pass:**
- `mcp.md`: Removed hardcoded port number — persona now instructs reading the spec.
- `lang_belta.md`: Fixed broken spec reference (`08_cinturonero_translator_protocol.md`
  → `14_cinturonero_translator_protocol.md`).
- `frontend.md`: Removed absolute `file:///home/butcherwutcher/...` paths.

---

### 4. rula.toml context_docs Cleanup

All agent `context_docs` lists trimmed to essentials: persona file + `AGENTS.md` (+ 
`TECHNICAL_DEBT.md` for audit/arch agents, + `docs/03_lang_belta_dictionary.md` for
lang_belta as genuine operational data). Specs and docs are now referenced inside the
persona file for on-demand reading.

---

### 5. Spec Update

`docs/specs/08_herder_agent_orchestration.md` section 8.1 updated to reflect:
- Reference injection pattern (replaces file-dump description).
- New `model` field and `/model` injection mechanism.

---

## Verification

- `cargo check`: 0 errors, 3 pre-existing warnings (unrelated to this change).
- No TypeScript changes — no `npm run build` required.

---

## Pre-existing Warnings (not introduced by this change)

- `src/watcher.rs:2` — unused import `std::fs`
- `src/kopeng/opendictate.rs:2` — unused import `std::io::BufRead`
- `src/hydration.rs:63` — unused constant `BASE_CONFIG_TOML`

These are tracked separately in `TECHNICAL_DEBT.md`.
