## PROJECT REMOVAL RESOLUTION
- Target Project: "{{PROJECT_NAME}}" (`{{PROJECT_PATH}}`)
- Flag ID: {{FLAG_ID}}
- Removal request and failure report (JSON: `purge_traces`, `failure_report`):

```json
{{METADATA_JSON}}
```

The user asked ONE-SHIP to remove this project. The automatic removal aborted before deleting anything, so the project is still registered. Your job is to fix what made it fail so the removal can complete.

### What the removal does
1. Exports the project's tasks, technical debt and session journal from SQLite to Markdown files in the project (`docs/` when it exists, otherwise the project root).
2. Only when `purge_traces` is `true`: deletes the ONE-SHIP artifacts from the project (managed files and directories, `.agents/` platform skills, runner MCP configs, the managed block of `AGENTS.md`) and the git refs under `refs/kapawu/snapshots/<project_id>/`.
3. Deletes the project's records from the ONE-SHIP database.

### TOOL CALLING MANDATE & OUTPUT CONSTRAINTS:
- All tools (`post_agent_response`, `complete_project_removal`) are native MCP tools provided by the `one-ship` MCP server. Invoke them through your environment's native tool caller.
- **STRICT PROHIBITION:** NEVER write Python scripts, bash curl commands, or shell scripts to make raw HTTP requests to `http://localhost:8765/mcp`.
- **TERMINAL STDOUT SILENCE:** Output printed to terminal stdout is NOT delivered to the user. Talk to the user only through `post_agent_response`.

### PROTOCOL
1. Read the failure report and diagnose each failure (permissions, files or directories that cannot be removed, locked git refs, missing paths).
2. Fix only what the failure report names. Do not edit, move or delete the project's own source code or documents. If a fix needs anything beyond that, or a failure cannot be fixed, explain it with `post_agent_response` and wait for the user's reply.
3. When every reported failure is fixed, call `complete_project_removal` with a `summary` of what failed and what you changed. The user sees a Delete approval:
   - Approved and the removal succeeds: the project is deleted and your session ends.
   - Approved and the removal fails again: you receive the new failure report; go back to step 1.
   - Rejected: the removal is cancelled and your session ends.
   - Any other text is feedback from the user; act on it, then call `complete_project_removal` again.
4. Never call `resolve_initialization_flag` for this flag.
