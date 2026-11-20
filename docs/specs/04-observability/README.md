# Observability Specifications

This directory specifies ONE-SHIP's persistence, discovery, and observability subsystems across agent work history, the development knowledge graph, and continuous run-bound process profiling.

## Specifications

| Document | Description |
| :--- | :--- |
| [Session Lifecycle, Database Devlog & Knowledge Graph](01-session-lifecycle-and-devlog-graph.md) | Tri-state agent session lifecycle, SQLite project journal/tasks/debt/init-flag schemas, MCP knowledge primitives, LanceDB + BGE-M3 + Tree-sitter semantic search, causal knowledge graph, and Omnisearch/Graph UI. |
| [Universal Telemetry, Continuous Profiling & Profiler Engines](02-universal-telemetry-and-profiling.md) | DuckDB continuous process-tree profiling bound to every supervised service and task run, run profile/comparison aggregation, declarative `manifest.toml` profiler engines, and missing-profiler flag resolution. |

## Reading Order

1. Start with [Session Lifecycle, Database Devlog & Knowledge Graph](01-session-lifecycle-and-devlog-graph.md) to understand how agent turns, session minutes, tasks, technical debt, code symbols, and Git hunks form a unified queryable project graph.
2. Continue with [Universal Telemetry, Continuous Profiling & Profiler Engines](02-universal-telemetry-and-profiling.md) to understand how supervised service and task executions are continuously sampled into DuckDB and wrapped by external profiler engines.
