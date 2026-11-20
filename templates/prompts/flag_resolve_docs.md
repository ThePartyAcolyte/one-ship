## INITIALIZATION FLAG RESOLUTION: DOCUMENTATION ARCHETYPE
- Target Project: "{{PROJECT_NAME}}" (`{{PROJECT_PATH}}`)
- Flag ID: {{FLAG_ID}}
- Selected Archetype: "{{CHOSEN_OPTION}}"
- Cleanup Strategy: "{{CLEANUP_STRATEGY}}"

### TOOL CALLING MANDATE & OUTPUT CONSTRAINTS:
- All tools (`post_agent_response`, `resolve_initialization_flag`) are native MCP tools provided by the `one-ship` MCP server.
- Invoke these tools directly via your environment's native tool caller (`call_mcp_tool` with ServerName `"one-ship"`, or direct tool wrappers).
- **STRICT PROHIBITION:** NEVER write Python scripts, bash curl commands, or shell scripts to make raw HTTP requests to `http://localhost:8765/mcp`.
- **TERMINAL STDOUT SILENCE:** Do NOT output final conversational dialogue or chat text directly to terminal stdout. Output printed to terminal stdout is NOT delivered to the user.
- **SINGLE CALL INVARIANT:** Call `post_agent_response` EXACTLY ONCE at the very conclusion of your turn when all scanning and drafting are complete. NEVER make dummy, ping, or probe calls.

---

### MULTI-TURN RESOLUTION PROTOCOL:

#### TURN 1 — RECONNAISSANCE & SCAFFOLDING PROPOSAL:
1. Inspect existing project structure and key source files.
2. Formulate the concrete draft of `docs/00_manifesto.md` and the living specification index under `docs/` according to the selected archetype (`{{CHOSEN_OPTION}}`).
3. If cleanup strategy is set to "purge", identify legacy documentation files to be archived or removed.
4. Deliver your findings and proposal by calling `post_agent_response` with the structured `approval` object:
   ```json
   {
     "dialogue": "I have prepared the living documentation manifesto and index according to the selected archetype. Please confirm to scaffold the files.",
     "artifact": "# Documentation Blueprint & Manifesto Draft\n\n...",
     "artifact_type": "markdown",
     "approval": {
       "title": "Living Documentation Scaffolding",
       "description": "Confirm scaffolding of living documentation files according to the chosen archetype.",
       "options": [
         "1. Proceed with documentation scaffolding",
         "2. Customize documentation manifesto draft"
       ]
     }
   }
   ```
5. **CRITICAL INVARIANT:** DO NOT invoke `resolve_initialization_flag` in Turn 1. Wait for the developer to select an option or reply in chat.

#### TURN 2+ — EXECUTION & CLOSURE:
1. When the developer confirms or provides feedback, write the documentation files into `docs/`.
2. Deliver a confirmation summary via `post_agent_response`.
3. Conclude the resolution by invoking `resolve_initialization_flag` with `flag_id: {{FLAG_ID}}`, `status: "resolved"`, and `chosen_option: "{{CHOSEN_OPTION}}"`.
