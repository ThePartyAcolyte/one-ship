## INITIALIZATION FLAG RESOLUTION: IN-VIVO PROFILER DRIVERS
- Target Project: "{{PROJECT_NAME}}" (`{{PROJECT_PATH}}`)
- Flag ID: {{FLAG_ID}}
- Stack Metadata: {{METADATA_JSON}}

### TOOL CALLING MANDATE & OUTPUT CONSTRAINTS:
- All tools (`post_agent_response`, `resolve_initialization_flag`) are native MCP tools provided by the `one-ship` MCP server. Profiling is continuous: a declared profiler runs with every start of its script, so there is no profiling tool to call.
- Invoke these tools directly via your environment's native tool caller (`call_mcp_tool` with ServerName `"one-ship"`, or direct tool wrappers).
- **STRICT PROHIBITION:** NEVER write Python scripts, bash curl commands, or shell scripts to make raw HTTP requests to `http://localhost:8765/mcp`.
- **TERMINAL STDOUT SILENCE:** Do NOT output final conversational dialogue or chat text directly to terminal stdout. Output printed to terminal stdout is NOT delivered to the user.
- **SINGLE CALL INVARIANT:** Call `post_agent_response` EXACTLY ONCE at the very conclusion of your turn when all scanning and formulation are complete. NEVER make dummy, ping, or probe calls.

---

### MULTI-TURN RESOLUTION PROTOCOL:

#### TURN 1 — RECONNAISSANCE & PROFILING PROPOSAL:
1. Inspect `{{PROJECT_PATH}}/rula.toml` to examine all configured `[scripts]`.
2. Inspect the project directory to determine the runtime, compiler, and language of each script (e.g. Rust, Node/TypeScript, Python, C/C++, Go).
3. Formulate a concrete Multi-Script Profiling Proposal:
   - Associate the optimal profiler driver per script (e.g. `cargo-flamegraph` for backend daemon scripts, `node` for web/CLI frontend scripts, `py-spy` for Python scripts).
   - If a custom profiler driver is needed, formulate the declarative `.agents/profiler/manifest.toml` recipe.
   - Detail any necessary host dependencies or build flags.
4. Deliver your findings and proposal by calling `post_agent_response` with the structured `approval` object:
   ```json
   {
     "dialogue": "I have analyzed the project scripts and prepared the in-vivo profiler configuration proposal. Please select your preferred driver setup.",
     "artifact": "# Profiler Configuration Proposal\n\n```toml\n[scripts.persistent.server.profiler]\ndriver = \"cargo-flamegraph\"\n```\n...",
     "artifact_type": "plan",
     "approval": {
       "title": "In-Vivo Profiler Driver Setup",
       "description": "Select how to configure profiler drivers for project scripts.",
       "options": [
         "1. Apply recommended profiler drivers to rula.toml",
         "2. Craft custom manifest.toml recipe",
         "3. Skip profiler driver configuration"
       ]
     }
   }
   ```
5. **CRITICAL INVARIANT:** DO NOT invoke `resolve_initialization_flag` in Turn 1. Wait for the developer to select an option or reply in chat.

#### TURN 2+ — EXECUTION & CLOSURE:
1. When the developer selects an option or provides feedback, write the updated `[scripts.<name>.profiler]` tables into `rula.toml` or generate `.agents/profiler/manifest.toml`.
2. Deliver a confirmation summary via `post_agent_response`.
3. Conclude the resolution by invoking `resolve_initialization_flag` with `flag_id: {{FLAG_ID}}`, and `status: "resolved"`.
