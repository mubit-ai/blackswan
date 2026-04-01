/// System prompt for the LLM memory selector.
pub fn recall_system_prompt() -> String {
    "You are selecting memories that will be useful to an AI agent as it processes \
     a user's query. You will be given the query and a list of available memory files.\n\n\
     Return a JSON object with a selected_memories array of filenames (up to 5). \
     Only include memories you are certain will be helpful. If unsure, don't include it. \
     Empty list is fine.\n\n\
     If recently-used tools are listed, do NOT select memories that are usage reference \
     for those tools (the agent is already using them). DO still select memories with \
     warnings, gotchas, or known issues about those tools.\n\n\
     Respond with ONLY valid JSON, no markdown fencing:\n\
     {\"selected_memories\": [\"file1.md\", \"file2.md\"]}"
        .to_string()
}

/// Build the user message for the recall LLM call.
pub fn recall_user_message(
    query: &str,
    manifest_text: &str,
    recently_used_tools: &[String],
) -> String {
    let mut msg = format!("## User Query\n{query}\n\n## Available Memories\n{manifest_text}");

    if !recently_used_tools.is_empty() {
        msg.push_str("\n\n## Recently Used Tools\n");
        for tool in recently_used_tools {
            msg.push_str(&format!("- {tool}\n"));
        }
    }

    msg
}
