---
name: oneship-desigma
description: Use the Desigma MCP (`desigma_*` tools) to create, inspect, animate, and synchronize framework-agnostic UI prototypes in `.desigma` files with real application code.
version: "0.2.6"
---

# ONE-SHIP Desigma Design Integration

This project has **Desigma** enabled. All design files live inside the project's configured `design/` directory and are exposed through the `desigma` MCP server using identical `desigma_*` tool names across all runners.

## Core Workflow (Design-First & Bidirectional Sync)

1. **Discover or Create a Design Document**
   - Call `desigma_list_files` to inspect existing `.desigma` / `.fig` files in the project's design workspace.
   - If starting from scratch, call `desigma_new_document` (or `desigma_open_file` on an existing file) and save changes with `desigma_save_file` (or edit live if the user has `desigma --bridge` open).

2. **Prototype & Animate**
   - Build screens and components with `desigma_create_page`, `desigma_create_node`, `desigma_set_appearance`, `desigma_set_text_style`, `desigma_move_node`, and `desigma_import_svg`.
   - For motion graphics and transitions, use `desigma_set_timeline`, `desigma_add_keyframe`, `desigma_apply_animation_preset`, and `desigma_export_animation`.

3. **Translate to Real Application Code (Framework-Agnostic)**
   - Inspect the scene graph with `desigma_get_document`, `desigma_list_nodes`, `desigma_get_node`, and `desigma_get_variables` (or `desigma_generate_code` as a reference).
   - Implement the UI in the project's actual target stack (Flutter, Kotlin Compose, SwiftUI, React, Vue, etc.).
   - **Node ID Provenance:** Annotate generated UI components with the Desigma `node_id` (e.g. `// @desigma-node: <node_id>`) so future visual edits in Desigma can be mapped back to the exact component without overwriting business logic.

4. **Synchronize Visual Adjustments**
   - When the user modifies a prototype in Desigma, re-read the affected nodes (`desigma_get_node` / `desigma_get_selection`) and update only the corresponding visual properties in the target codebase, preserving all event handlers and state logic.
