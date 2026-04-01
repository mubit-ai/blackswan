use crate::types::Memory;

/// System prompt for the extraction LLM agent.
pub fn extraction_system_prompt() -> String {
    r#"You are a memory extraction agent for an AI assistant. Your job is to analyze conversation messages and decide what memories to create, update, or delete.

## Memory Types
- **user**: Role, goals, preferences, expertise level, collaboration style
- **feedback**: Corrections the user made, approaches confirmed to work (structure: Rule → Why → How to apply)
- **project**: Ongoing initiatives, deadlines, architecture decisions (structure: Fact → Why → How to apply)
- **reference**: Pointers to external systems (system name + URL/path + purpose)

## What NOT to save
- Code patterns, architecture, file paths (derivable from source)
- Git history (git log is authoritative)
- Debugging solutions (the fix is in the code)
- Anything already in project instruction files
- Ephemeral task details or temporary state

## Rules
- Check existing memories before creating — update if a relevant memory exists
- Convert relative dates to absolute dates
- Be specific in descriptions (they're used for recall relevance matching)
- Only extract information that will be useful in future conversations

## Response Format
Respond with ONLY valid JSON (no markdown fencing):
{
  "actions": [
    {
      "action": "create",
      "name": "descriptive name",
      "description": "one-line description for recall matching",
      "type": "user|feedback|project|reference",
      "content": "markdown content"
    },
    {
      "action": "update",
      "filename": "existing_file.md",
      "name": "updated name",
      "description": "updated description",
      "type": "user|feedback|project|reference",
      "content": "updated markdown content"
    },
    {
      "action": "delete",
      "filename": "obsolete_file.md"
    }
  ]
}

If there is nothing worth extracting, return: {"actions": []}"#.to_string()
}

/// Build the user message for extraction, including conversation and existing memory manifest.
pub fn extraction_user_message(
    messages: &[crate::types::Message],
    existing_memories: &[Memory],
) -> String {
    let mut msg = String::from("## Conversation\n");

    for m in messages {
        msg.push_str(&format!("[{}] {}\n\n", m.role_str(), m.content));
    }

    if !existing_memories.is_empty() {
        msg.push_str("\n## Existing Memories\n");
        for mem in existing_memories {
            msg.push_str(&format!(
                "- [{}] {} ({}): {}\n",
                mem.memory_type, mem.path.file_name().unwrap_or_default().to_string_lossy(),
                mem.name, mem.description
            ));
        }
    }

    msg
}

impl crate::types::Message {
    fn role_str(&self) -> &str {
        match self.role {
            crate::types::MessageRole::User => "user",
            crate::types::MessageRole::Assistant => "assistant",
            crate::types::MessageRole::System => "system",
        }
    }
}
