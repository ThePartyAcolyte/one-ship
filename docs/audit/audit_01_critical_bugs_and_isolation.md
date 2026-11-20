# Audit 01 — Critical Bugs, Data Corruption Risks & Cross-Project Isolation Gaps
**Audit Date:** `2026-11-16` | **Severity:** 🔴 CRITICAL / 🟠 HIGH  
**Governance:** Every item below must be reviewed and approved via a Technical Proposal before applying fixes.

---

## Part 1: Logic & Data Integrity Bugs (`B-01` – `B-08`)

### 🔴 `B-01`: Cross-Project Primary Key Collisions in LanceDB (`semantic/chunker.rs`)
- **Location:**
  - [`src-tauri/src/semantic/chunker.rs:65, 90`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/semantic/chunker.rs#L65-L90) (`chunk_code_file`)
  - [`src-tauri/src/semantic/chunker.rs:174, 199`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/semantic/chunker.rs#L174-L199) (`chunk_markdown_spec`)
  - [`src-tauri/src/semantic/store.rs:139-142`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/semantic/store.rs#L139-L142) (`upsert_records`)
- **Problem:**
  1. Code chunk IDs are constructed as `format!("code::{}::{}", relative_path, name)` (omitting both `project_id` and `line_start`).
  2. Markdown spec chunk IDs are constructed as `format!("specs::{}#{}", relative_path, current_header)` (omitting `project_id` and `line_start`).
  3. Because all projects share a single global LanceDB table (`knowledge`) and `upsert_records` executes `table.delete(&format!("id = '{}'", safe_id))` before inserting, indexing Project B overwrites any chunk from Project A that shares the same relative path and symbol/header name (e.g. `code::src/main.rs::main` or `specs::docs/README.md#Overview`).
  4. Within the *same* file, multiple `impl Foo` blocks produce identical `name = "Foo"` and overwrite each other.
  5. By contrast, `journal`, `tasks`, and `tech_debt` chunks in [`src-tauri/src/semantic/mod.rs:102, 232, 253, 272`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/semantic/mod.rs#L232-L272) include `project_id` (`journal::{project_id}::{id}`).
- **Proposed Fix Options (for WP-2 Review):**
  - Include `project_id` and `line_start` in all code and spec chunk IDs: `format!("code::{}::{}::{}::{}", project_id, relative_path, line_start, name)` and `format!("specs::{}::{}::{}#{}", project_id, relative_path, line_start, current_header)`.

---

### 🔴 `B-02`: 100% No-Op JIT Line Resolution Performing Synchronous Disk Reads (`semantic/mod.rs`)
- **Location:** [`src-tauri/src/semantic/mod.rs:63-74`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/semantic/mod.rs#L63-L74) (`search_knowledge`)
- **Problem:**
  ```rust
  if let Ok(file_content) = std::fs::read_to_string(path) {
      if let Some(pos) = file_content.lines().position(|l| l.contains(&item.title)) {
          let new_start = (pos + 1) as u32;
  ```
  1. `item.title` is generated in `chunker.rs` as `"{symbol_name} in {relative_path}"` (line 69), `"{relative_path} (lines {start}-{end})"` (line 117), or `"{relative_path} (§{header})"` (line 177).
  2. No line inside a source file or markdown document will ever contain `"fn_name in src/foo.rs"` or `"docs/spec.md (§Header)"`.
  3. Consequently, this loop performs synchronous `std::fs::read_to_string` on the Tokio async worker thread for every search hit while matching `0%` of the time.
- **Proposed Fix Options (for WP-2 Review):**
  - Extract the first non-empty line of `item.content_preview` (the actual code signature or markdown heading line) to locate line drift, and run file reads inside `spawn_blocking`.

---

### 🔴 `B-03`: 16-Byte Buffer Truncation Breaking ELF Architecture Classification in REA (`kopeng/rea.rs`)
- **Location:** [`src-tauri/src/kopeng/rea.rs:1105-1125`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/kopeng/rea.rs#L1105-L1125) (`classify_binary_magic` & `classify_header`)
- **Problem:**
  - Line 1105 allocates a 16-byte buffer: `let mut header = [0u8; 16];`.
  - Lines 1112 and 1121 read the ELF `e_machine` field at byte offsets 18 and 19:
    `let machine = u16::from_le_bytes([*header.get(18).unwrap_or(&0), *header.get(19).unwrap_or(&0)]);`
  - Because `header.len() == 16`, `header.get(18)` and `header.get(19)` always return `None` (`0`), so `machine` is always `0`. Every ELF binary (`x86_64`, `x86`, `ARM32`, `AArch64`, `RISC-V`) always falls through to the generic `"ELF"` label and never reports its CPU architecture.
- **Proposed Fix Options (for WP-2 Review):**
  - Increase `header` buffer in `classify_binary_magic` from `[0u8; 16]` to `[0u8; 20]` (or `[0u8; 64]`, the full 64-bit ELF header size).

---

### 🔴 `B-04`: Missing Project Ownership Verification on 9 MCP Tools (`mcp.rs` & `db.rs`)
- **Location:**
  - [`src-tauri/src/mcp.rs:2141-2196`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/mcp.rs#L2141-L2196) (`execute_query_project_task`)
  - [`src-tauri/src/mcp.rs:2198-2243`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/mcp.rs#L2198-L2243) (`execute_cancel_project_task`)
  - [`src-tauri/src/mcp.rs:2976-3014`](file:///home/butcherwutcher/Projects/dev/one-ship/src/mcp.rs#L2976-L3014) (`execute_resolve_document_comment`)
  - [`src-tauri/src/mcp.rs:3084-3117`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/mcp.rs#L3084-L3117) (`execute_manage_todo_item` — `update_status`, `complete`, `delete`)
  - [`src-tauri/src/mcp.rs:3643-3681`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/mcp.rs#L3643-L3681) (`execute_resolve_todo_item`)
  - [`src-tauri/src/mcp.rs:3605-3629`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/mcp.rs#L3605-L3629) (`execute_record_technical_debt` — `resolve`)
  - [`src-tauri/src/mcp.rs:3683-3720`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/mcp.rs#L3683-L3720) (`execute_resolve_technical_debt`)
  - [`src-tauri/src/mcp.rs:3880-3889`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/mcp.rs#L3880-L3889) (`execute_get_run_profile`)
  - [`src-tauri/src/mcp.rs:3891-3905`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/mcp.rs#L3891-L3905) (`execute_compare_profiler_runs`)
- **Problem:**
  - By contrast with `execute_manage_project_stage`, `execute_manage_project_task`, and `execute_manage_task_subtask` (which strictly pass `p.id` into `db_*` and verify `project_id` ownership), the 9 tools above mutate or read records by global primary key (`todo_id`, `debt_id`, `comment_id`, `exec_id`, `run_id`) without verifying that the record belongs to the caller's resolved project `p.id`.
  - Furthermore, `db_update_todo_status` ([`db.rs:3895-3909`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/db.rs#L3895-L3909)), `db_delete_project_todo` ([`db.rs:5523-5534`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/db.rs#L5523-L5534)), `db_resolve_technical_debt` ([`db.rs:4001-4011`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/db.rs#L4001-L4011)), and `db_resolve_document_comment` ([`db.rs:5342-5360`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/db.rs#L5342-L5360)) do not check `WHERE project_id = ?` and do not check `rows_affected == 0`, silently returning `Ok(())` even when given a non-existent or cross-project ID.
- **Proposed Fix Options (for WP-2 Review):**
  - Require `project_id` in the `WHERE` clause of `db_update_todo_status`, `db_delete_project_todo`, `db_resolve_technical_debt`, and `db_resolve_document_comment`, return `DatabaseError::QueryFailed` / not-found when `rows_affected == 0`, and verify project ownership in `execute_query_project_task`, `execute_cancel_project_task`, `execute_get_run_profile`, and `execute_compare_profiler_runs`.

---

### 🔴 `B-05`: Destructive `rula.toml` Overwrite & Wrong Table Path in Electronics (`kopeng/electronics/`)
- **Location:**
  - [`src-tauri/src/kopeng/electronics/mod.rs:138-162`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/kopeng/electronics/mod.rs#L138-L162) (`toggle_project_electronics_plugin`)
  - [`src-tauri/src/kopeng/electronics/firmware.rs:389-396`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/kopeng/electronics/firmware.rs#L389-L396) (`scaffold_firmware_project`)
- **Problem:**
  1. In `toggle_project_electronics_plugin`:
     ```rust
     let mut doc: toml::Value = match crate::config::read_project_rula(p_path) {
         Ok(Some(v)) => v,
         _ => toml::Value::Table(Default::default()),
     };
     ```
     If `rula.toml` has a syntax error or transient read failure, `read_project_rula` returns `Err`, which the wildcard `_` arm replaces with an empty table and writes back to disk on line 162 — **wiping the entire project's `rula.toml` configuration** (services, tasks, personas, plugins). Every other plugin toggle (`profiler.rs:468`, `rea.rs:1408`, `desigma.rs:553`, `solidcraft.rs:567`, `pcbcraft.rs:679`) propagates `read_project_rula` errors with `?`.
  2. In `scaffold_firmware_project` (`firmware.rs:389`):
     Writes `val.as_table_mut().entry("electronics")` (`[electronics] enabled = true` at root level) instead of `[plugins.electronics] enabled = true` (which `is_project_electronics_enabled` in `electronics/mod.rs:51` reads).
  3. Also in `scaffold_firmware_project` (`firmware.rs:296-298, 362, 367, 377, 381, 395`): chains `.await.ok()` on directory and file creation calls, reporting `success: true` even when disk writes fail.
- **Proposed Fix Options (for WP-2 Review):**
  - Propagate `read_project_rula` errors via `?` and reject uninitialized projects (`Ok(None)`) in `toggle_project_electronics_plugin` (and `project_mgmt.rs:358`); fix `firmware.rs:389` to write under `[plugins.electronics]`; propagate I/O errors in `scaffold_firmware_project`.

---

### 🟠 `B-06`: Incomplete De-Integration of `desigma` & `pcbcraft` MCP Entries (`kopeng/manifest.rs`)
- **Location:** [`src-tauri/src/kopeng/manifest.rs:16`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/kopeng/manifest.rs#L16) vs [`src-tauri/src/kopeng/skill.rs:418-467`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/kopeng/skill.rs#L418-L467)
- **Problem:**
  - `skill::inject_project_mcp_plugin` injects 4 MCP server keys into `<project>/.mcp.json` and `<project>/opencode.json`: `"one-ship"`, `"rea"`, `"desigma"`, and `"pcbcraft"`.
  - However, `ONESHIP_MCP_SERVER_KEYS` in `manifest.rs:16` is `&["one-ship", "rea"]`. When a project is de-integrated ("Space Him" / `purge_oneship_artifacts`), `"desigma"` and `"pcbcraft"` are left behind in `.mcp.json` and `opencode.json`.
- **Proposed Fix Options (for WP-2 Review):**
  - Update `ONESHIP_MCP_SERVER_KEYS` in `manifest.rs:16` to `&["one-ship", "rea", "desigma", "pcbcraft"]`.

---

### 🟠 `B-07`: Cross-Project Hyprland Window Class Collision (`desigma.rs`, `solidcraft.rs`, `pcbcraft.rs`)
- **Location:**
  - [`src-tauri/src/kopeng/desigma.rs:505-518, 647-660`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/kopeng/desigma.rs#L505-L518)
  - [`src-tauri/src/kopeng/solidcraft.rs:543-548, 621-630`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/kopeng/solidcraft.rs#L543-L548)
  - [`src-tauri/src/kopeng/pcbcraft.rs:657-659, 866-871`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/kopeng/pcbcraft.rs#L657-L659)
- **Problem:**
  - When `DESIGMA_PROCESSES` / `SOLIDCRAFT_PROCESSES` / `PCBCRAFT_PROCESSES` has no entry for `project_path`, `is_editor_process_alive` and `launch_project_*` fall back to `find_hyprland_client_pid_by_class(&["desigma", ...])`, which matches **any** open window of that class across the entire desktop without checking `/proc/<pid>/cwd` or `/proc/<pid>/cmdline`.
  - If Project A has `pcbcraft` open, opening Project B's PCB tab shows `Editor Running` and clicking `Launch` focuses Project A's window instead of launching `pcbcraft` for Project B.
- **Proposed Fix Options (for WP-2 Review):**
  - Verify that any candidate PID returned by `hyprctl clients -j` has `/proc/<pid>/cmdline` or `/proc/<pid>/cwd` referencing the target `project_path` before adopting/focusing it.

---

### 🟠 `B-08`: Broken `event.preventDefault()` After `await` & Synthetic DOM Fallback in Paste Handler (`FloatingChatOverlay.vue`)
- **Location:** [`src/components/dialogue/FloatingChatOverlay.vue:314-378`](file:///home/butcherwutcher/Projects/dev/one-ship/src/components/dialogue/FloatingChatOverlay.vue#L314-L378) (`handlePaste`)
- **Problem:**
  1. Line 342 executes `const savedPath = await explorerStore.pasteWaylandClipboardImage(projectPath);` and then calls `event.preventDefault();` on line 344. In browser event dispatch, calling `preventDefault()` asynchronously after an `await` is a no-op because default paste handling has already completed synchronously.
  2. Lines 348–377 fall back to browser `event.clipboardData` + `FileReader` base64 conversion, violating `AGENTS.md §6.2` (Wayland native `wl-paste` integration without browser synthetic fallbacks).
- **Proposed Fix Options (for WP-2 Review):**
  - Check if `event.clipboardData` contains non-text/image items or handle image paste via `wl-paste` cleanly, calling `event.preventDefault()` synchronously when intercepting an image paste.

---

## Part 2: Portrait DNA & Manifest Bugs (`Spec 21`)

### 🟠 `B-09`: Broken Portrait Expressions, Gender Inversion & Out-of-Bounds Asset IDs
- **Location:**
  - [`scripts/build_portrait_catalog.mjs:40-49`](file:///home/butcherwutcher/Projects/dev/one-ship/scripts/build_portrait_catalog.mjs#L40-L49) & [`public/portraits/manifest.json:94-125`](file:///home/butcherwutcher/Projects/dev/one-ship/public/portraits/manifest.json#L94-L125)
  - [`src-tauri/src/kopeng/dna.rs:11-29`](file:///home/butcherwutcher/Projects/dev/one-ship/src-tauri/src/kopeng/dna.rs#L11-L29)
  - [`src/components/ui/AppAgentPortrait.vue:63-79, 132-135`](file:///home/butcherwutcher/Projects/dev/one-ship/src/components/ui/AppAgentPortrait.vue#L63-L79)
  - [`src/components/modals/AddPersonaModal.vue:166-175`](file:///home/butcherwutcher/Projects/dev/one-ship/src/components/modals/AddPersonaModal.vue#L166-L175)
- **Problem:**
  1. `manifest.json` `expression` items (`E0`–`E5`) have no `path` field, so `if (exprAsset?.path)` in `AppAgentPortrait.vue:132` is always `false` and expressions are never drawn.
  2. `dna.rs:11-27` treats `B01`–`B04` as feminine (`bd_idx = 0`) and `B05`–`B14` as masculine, whereas in `manifest.json:8-93`, `B01`–`B04` are masculine (`Human1..4.png`) and `B05, B06, B11, B12` are feminine (causing feminine bases to receive beards and masculine bases `B01`–`B04` to never receive beards).
  3. `dna.rs:20-29` generates `H22`, `BD06`, and `A17`, and `AddPersonaModal.vue:166-175` / `AppAgentPortrait.vue:63-79` generate `B15`–`B20` and `BD06`–`BD17`, none of which exist in `manifest.json` (`B01`–`B14`, `BD00`–`BD05`, `H00`–`H21`, `A00`–`A16`).
