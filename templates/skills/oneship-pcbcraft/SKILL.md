---
name: oneship-pcbcraft
description: Use the PCBCraft MCP (`pcbcraft` server) to search parts, design and verify schematics, inspect netlists, and run DRC checks without performing manual copper trace routing.
version: "0.2.6"
---

# ONE-SHIP PCBCraft EDA Workflow Guidelines

This project has **PCBCraft** enabled (`[plugins.pcbcraft]`). All EDA project folders and `.pc` archives live inside the configured `pcb/` workspace directory and are exposed to agents through the `pcbcraft` MCP gateway (`/mcp/pcbcraft`).

## Scope & Division of Responsibilities

1. **Architecture, BOM & Schematics (Agent Allowed):**
   - Inspect current circuit state with `inspect_project`.
   - Discover parts, symbols, and footprints with `list_commands` / `describe_command` and library commands (`library.search`, `library.resolve_part`, `library.list`).
   - Author or update schematic topology using `execute` with `schematic.*` commands (placing components, connecting pins with wires/net labels, editing component values and simulation attributes such as `exclude_from_simulation`).
   - All `execute` edits accumulate in memory until persisted: **ALWAYS call `save_project` at the end of your edit batch** so changes are written to disk (or saved in the open PCBCraft editor) and a pre-save safety snapshot is captured.
   - Validate design rules with `execute("pcb.check_design")` after schematic or board edits.
   - After saving a batch of edits, briefly summarize what changed and ask the user whether they want to review it in PCBCraft, keep it, or revert to the automatic snapshot if needed.

2. **Strict No-LLM Copper Routing Guardrail (PROHIBITED):**
   - **NEVER** invoke low-level PCB copper geometry or trace-routing commands via MCP (`pcb.add_tracks`, `pcb.add_via`, `pcb.reshape_tracks`, `pcb.move_via`, or manual polygon trace editing).
   - Physical trace layout, differential pair routing, and manual copper pouring belong to the human engineer inside the external `pcbcraft` desktop editor or dedicated deterministic autorouters.
   - If copper routing or DRC trace fixes are needed, report the exact netlist/DRC findings clearly and ask the user to open PCBCraft or run the dedicated routing workflow.

3. **SPICE & Electrical Simulation Readiness:**
   - Ensure power symbols (`+3V3`, `GND`, etc.) and reference designators (`R*`, `C*`, `L*`, `U*`, `D*`, `J*`) carry accurate engineering values (e.g., `4.7k`, `100nF`, `10uH`) so downstream SPICE netlist extraction produces valid electrical models.
