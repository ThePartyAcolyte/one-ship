## PROJECT CONFIGURATION REPAIR
- Target Project: "{{PROJECT_NAME}}" (`{{PROJECT_PATH}}`)
- Flag ID: {{FLAG_ID}}

ONE-SHIP found the project's `rula.toml` invalid: it does not parse as TOML, a registered setting has a value of the wrong type, or a runner/model/effort pair is not one its runner declares (an `[agents.<id>]` table without `runner` and `model`, a model of another runner or one removed from the runner, an unsupported effort, or a `[plugins.orchestrator]` runner set without its model). Until it is fixed, ONE-SHIP ignores every project setting in it (scripts, agent personas, plugin overrides) and runs the project on the global configuration. The exact error is in the flag description shown to the user and is reproduced by reading the file.

### TOOL CALLING MANDATE & OUTPUT CONSTRAINTS:
- `post_agent_response` is a native MCP tool provided by the `one-ship` MCP server. Invoke it through your environment's native tool caller.
- **STRICT PROHIBITION:** NEVER write Python scripts, bash curl commands, or shell scripts to make raw HTTP requests to `http://localhost:8765/mcp`.
- **TERMINAL STDOUT SILENCE:** Output printed to terminal stdout is NOT delivered to the user. Talk to the user only through `post_agent_response`.

### PROTOCOL
1. Read `rula.toml` and locate the error: a TOML syntax error (line and column), or a setting under `[system]` / `[plugins.<id>]` whose type does not match its schema (boolean, number, string, one of an enumerated list, list of strings).
2. Consult the `oneship-core` skill reference `references/rula_schema.md` for the expected tables and value types. Runner/model/effort pairs come ONLY from this catalog, generated from the ONE-SHIP runner manifests; never choose a model from memory. To repair an obsolete or mismatched model, ask the user which listed model of that runner to use (an `approval` with the candidates) unless the file makes the intent unambiguous:
{{RUNNER_CATALOG}}
3. Fix only what makes the file invalid. Keep every other table, key, comment and value unchanged. If the intended value cannot be inferred, ask the user with `post_agent_response` (include an `approval` with the candidate values) before editing.
4. After the fix, report the change with `post_agent_response`. ONE-SHIP re-validates the file on its next watcher tick and closes the flag by itself; never call `resolve_initialization_flag` for it.
