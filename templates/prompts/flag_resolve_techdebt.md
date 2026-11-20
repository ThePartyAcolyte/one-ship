## INITIALIZATION FLAG RESOLUTION: ARCHITECTURAL TECHNICAL DEBT
- Target Project: "{{PROJECT_NAME}}" (`{{PROJECT_PATH}}`)
- Flag ID: {{FLAG_ID}}

### TOOL CALLING MANDATE & OUTPUT CONSTRAINTS:
- All tools (`record_technical_debt`, `post_agent_response`, `resolve_initialization_flag`) are native MCP tools provided by the `one-ship` MCP server.
- Invoke these tools directly via your environment's native tool caller (`call_mcp_tool` with ServerName `"one-ship"`, or direct tool wrappers).
- **STRICT PROHIBITION:** NEVER write Python scripts, bash curl commands, or shell scripts to make raw HTTP requests to `http://localhost:8765/mcp`.
- **TERMINAL STDOUT SILENCE:** Do NOT output final conversational dialogue or chat text directly to terminal stdout. Output printed to terminal stdout is NOT delivered to the user.
- **SINGLE CALL INVARIANT:** Call `post_agent_response` EXACTLY ONCE at the very conclusion of your turn when all scanning and staging are complete. NEVER make dummy, ping, or probe calls.

---

### MULTI-TURN RESOLUTION PROTOCOL:

#### TURN 1 — RECONNAISSANCE, RECORD STAGING & PROPOSAL:
1. Inspect project reconnaissance notes, temporary shims, and legacy `TECHNICAL_DEBT.md` files if present.
2. Formulate classified debt entries (category: `architectural` | `performance` | `maintainability`, severity: `low` | `medium` | `high` | `critical`).
3. Stage each debt item into SQLite using the `record_technical_debt` MCP tool (`action: "record"`).
4. Deliver your findings and proposal by calling `post_agent_response` with the structured `approval` object:
   ```json
   {
     "dialogue": "I have scanned the project and recorded the technical debt items into SQLite. Please choose how you would like to handle legacy debt records.",
     "artifact": "# Technical Debt Inventory\n\n| ID | Category | Severity | Description | Remediation Plan |\n|---|---|---|---|---|\n...",
     "artifact_type": "plan",
     "approval": {
       "title": "Technical Debt Management Strategy",
       "description": "Select how to manage legacy technical debt records across the repository.",
       "options": [
         "1. Archive legacy debt files into SQLite ledger",
         "2. Keep legacy markdown synchronized",
         "3. Preserve legacy files as read-only reference"
       ]
     }
   }
   ```
5. **CRITICAL INVARIANT:** DO NOT invoke `resolve_initialization_flag` in Turn 1. Wait for the developer to select an option or reply in chat.

#### TURN 2+ — EXECUTION & CLOSURE:
1. When the developer selects an option or provides feedback, execute any requested file updates or archival.
2. Deliver a confirmation summary via `post_agent_response`.
3. Conclude the resolution by invoking `resolve_initialization_flag` with `flag_id: {{FLAG_ID}}`, and `status: "resolved"`.
