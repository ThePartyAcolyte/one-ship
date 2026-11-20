---
name: oneship-docstyle-contract
description: Contract-First API & Interface documentation standard.
version: 0.2.5
---

# Contract-First Documentation Standard

The Contract-First archetype centers documentation around explicit API schemas, IPC messages, and protocols.

## 1. Directory Structure
```text
docs/
├── contracts/                    # Formal API schemas (OpenAPI, JSON-RPC, Protobuf)
│     ├── ipc_protocol.json
│     └── http_endpoints.md
└── architecture/
      └── system_topology.md
```

## 2. Invariant Rules
- Schema contracts serve as Single Source of Truth (SSOT).
- All message payloads, events, and RPC commands must strictly adhere to documented schemas.
