# Hardware, CAD, Design, and Reverse Engineering

Specifications for ONE-SHIP's physical engineering, visual design, parametric CAD, EDA/PCB, and binary reverse-engineering subsystems.

## Reading Order

1. [Electronics Workstation and Instrumentation Engine](01-electronics-workstation-and-instrumentation.md) — Native USB drivers (Owon VDS1022, FX2LP logic analyzer), hardware lock arbitration, virtual benchtop UI, firmware scaffolding, EDA ingestion, and headless SPICE simulation.
2. [One-Ship Designer Integration](02-oneship-designer-integration.md) — Sibling `desigma` fork topology, upstream sync workflow, preview rendering cache, and per-project `/mcp/desigma` gateway.
3. [CAD and PCB Integration](03-cad-and-pcb-integration.md) — SolidCraft 3D CAD preview/metrics, PCBCraft schematic/board authoring and `/mcp/pcbcraft` gateway, SPICE/EIS netlist bridge, and sourcing/fabrication roadmaps.
4. [REA Reverse Engineering Engine and Multi-Project Gateway](04-rea-reverse-engineering-integration.md) — Per-project supervised `rea` HTTP engine, `/mcp/rea` FIFO heavy-job queue, idle reaper, and visual RE Lab explorer.
