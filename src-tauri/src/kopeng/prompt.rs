/// Builds the initial startup directives and system prompt for an agent persona.
pub fn build_agent_initial_prompt(
    agent_id: &str,
    system_prompt: Option<&str>,
    mcp_dialogue_mode: bool,
    context_docs: &[String],
    skills: &[String],
) -> Option<String> {
    let mut initial_prompt_parts = Vec::new();

    if let Some(prompt) = system_prompt {
        let trimmed = prompt.trim();
        if !trimmed.is_empty() {
            initial_prompt_parts.push(trimmed.to_string());
        }
    }

    let clean_skills: Vec<&str> = skills
        .iter()
        .map(|s| s.as_str())
        .filter(|s| !crate::kopeng::skill::PLATFORM_BASE_SKILLS.contains(s))
        .collect();

    if !clean_skills.is_empty() {
        initial_prompt_parts.push(format!(
            "## Assigned Domain Skills\n\
             The following specialized project skills are assigned to your persona:\n\
             {}\n\
             When planning, reasoning, or executing tasks within your domain, prioritize and consult these skills.",
            clean_skills.iter().map(|s| format!("- `{}`", s)).collect::<Vec<_>>().join("\n")
        ));
    }

    if mcp_dialogue_mode {
        initial_prompt_parts.push(format!(
            "## Response Protocol (mcp_dialogue_mode active)\n\n\
             Your terminal stream is a reasoning and execution log — not an interactive chat surface.\n\n\
             You operate in headless background dialogue mode as agent `{agent_id}`. The user's first message follows this preamble; do not greet or announce yourself, simply answer it.\n\n\
             RULES:\n\
             - You MUST end every user response with a call to `post_agent_response` (ONE-SHIP identifies you and your project automatically; never pass your own name or project).\n\
               * `dialogue`: plain conversational text (read aloud via TTS). No markdown code blocks.\n\
               * `artifact`: relative file path, code, diff, plan, or markdown document (optional).\n\
             - JOURNAL & CONTINUITY DIRECTIVES:\n\
               * MANDATORY (primary/orchestrating agents): Whenever you complete a milestone, implement a feature, fix a bug, modify files, or make architectural decisions, you MUST record your work via `record_session_minutes` (concise <= 72 char title) before concluding with `post_agent_response`. Delegated specialists executing a pending task from another agent must NOT call `record_session_minutes`; report via `report_task_status` instead.\n\
               * To inspect past milestones and previous minutes, call `query_session_journal`.\n\
             - SCRIPT & SERVICE LIFECYCLE DIRECTIVES:\n\
               * NEVER run project build, test, lint, or persistent dev commands via raw shell/bash commands if they exist in the project catalog (`rula.toml`).\n\
               * Check available scripts with `list_project_scripts`.\n\
               * Execute finite tasks (tests, builds, linter) via `start_project_task(task_id: \"...\", wait: true)`.\n\
               * Manage persistent background services (dev servers, watchers) via `manage_project_script(script_id: \"...\", action: \"start\"|\"stop\"|\"restart\")`.\n\
             - LIVE ARTIFACTS & SCRATCHPAD DIRECTIVES:\n\
               * When formulating detailed plans, architecture notes, complex task breakdowns, or research summaries, write them directly to disk:\n\
                 - `scratchpad/<name>.md` for working drafts, temporary plans, and conversation-scoped scratch notes.\n\
                 - `docs/<name>.md` for permanent project specifications, architecture notes, and documentation.\n\
               * In `post_agent_response`, pass the relative path in `artifact` (e.g. \"artifact\": \"scratchpad/plan.md\").\n\
               * When you subsequently edit this file with file edit tools, the user's viewer panel auto-refreshes directly from disk. Do not re-dump full document text into the chat dialogue.\n\
             - For any destructive action, unexpected command, or file edit not explicitly requested, call `request_approval` BEFORE executing.\n\
             - NEVER write conversational text directly to the terminal."
        ));
    } else if !context_docs.is_empty() {
        let refs: Vec<String> = context_docs
            .iter()
            .map(|p| format!("- {}", p))
            .collect();
        initial_prompt_parts.push(format!(
            "[CONTEXT REFERENCE]\nThe following files define your working context for this session.\nRead them proactively as needed to fulfill your role:\n\n{}",
            refs.join("\n")
        ));
    }

    if initial_prompt_parts.is_empty() {
        None
    } else {
        Some(initial_prompt_parts.join("\n\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_prompt_mcp_dialogue() {
        let p = build_agent_initial_prompt("bosmang", Some("You are dev lead"), true, &[], &[]);
        assert!(p.is_some());
        let content = p.unwrap();
        assert!(content.contains("You are dev lead"));
        assert!(content.contains("Response Protocol (mcp_dialogue_mode active)"));
        assert!(content.contains("agent `bosmang`"));
        assert!(!content.contains("project_uid"));
        assert!(!content.contains("STARTUP DIRECTIVES"));
        assert!(!content.contains("greeting stating your role"));
        assert!(!content.contains("`agent_id`"));
    }
}
