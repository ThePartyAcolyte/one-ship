## INITIALIZATION FLAG RESOLUTION: INLINE TASKS & TODOS MIGRATION
- Target Project: "{{PROJECT_NAME}}" (`{{PROJECT_PATH}}`)
- Flag ID: {{FLAG_ID}}

### TOOL CALLING MANDATE & OUTPUT CONSTRAINTS:
- All tools (`manage_todo_item`, `post_agent_response`, `resolve_initialization_flag`) are native MCP tools provided by the `one-ship` MCP server.
- Invoke these tools directly via your environment's native tool caller (`call_mcp_tool` with ServerName `"one-ship"`, or direct tool wrappers).
- **STRICT PROHIBITION:** NEVER write Python scripts, bash curl commands, or shell scripts to make raw HTTP requests to `http://localhost:8765/mcp`.
- **TERMINAL STDOUT SILENCE:** Do NOT output final conversational dialogue or chat text directly to terminal stdout. Output printed to terminal stdout is NOT delivered to the user.
- **SINGLE CALL INVARIANT:** Call `post_agent_response` EXACTLY ONCE at the very conclusion of your turn when all scanning and staging are complete. NEVER make dummy, ping, or probe calls.

---

### MULTI-TURN RESOLUTION PROTOCOL:

#### TURN 1 — RECONNAISSANCE, RECORD STAGING & PROPOSAL:
1. Scan source code files across `{{PROJECT_PATH}}` for inline `TODO:`, `FIXME:`, `HACK:`, and `BUG:` tags.
2. Stage discovered tasks into SQLite using the `manage_todo_item` MCP tool (`action: "create"`).
3. Deliver your findings and proposal by calling `post_agent_response` with the structured `approval` object:
   ```json
   {
     "dialogue": "I have scanned the project for inline tasks and staged them into SQLite. Please choose how you want to handle the inline code comments.",
     "artifact": "# Discovered Tasks Inventory\n\n| Title | File | Line | Inferred Priority |\n|---|---|---|---|\n...",
     "artifact_type": "plan",
     "approval": {
       "title": "Inline Tasks Migration Strategy",
       "description": "Select how to handle legacy inline comment markers across the codebase.",
       "options": [
         "1. Purge inline comments from codebase",
         "2. Annotate comments with SQLite task IDs",
         "3. Preserve inline comments in code as-is"
       ]
     }
   }
   ```
4. **CRITICAL INVARIANT:** DO NOT invoke `resolve_initialization_flag` in Turn 1. Wait for the developer to select an option or reply in chat.

#### TURN 2+ — EXECUTION & CLOSURE:
1. When the developer selects an option or provides feedback, execute the required file modifications according to their choice.
2. Deliver a concise confirmation report via `post_agent_response`.
3. Conclude the resolution by invoking `resolve_initialization_flag` with `flag_id: {{FLAG_ID}}`, and `status: "resolved"`.
