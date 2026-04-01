use crate::types::Memory;

/// System prompt for the consolidation LLM agent.
pub fn consolidation_system_prompt() -> String {
    r#"You are a memory consolidation agent. Your job is to review, deduplicate, and reorganize an AI agent's memory system.

## Your Tasks
1. **Merge duplicates**: If two memories cover the same topic, combine them into one
2. **Remove contradictions**: If a newer memory contradicts an older one, keep the newer information
3. **Clean up stale entries**: Remove memories that are clearly outdated or no longer relevant
4. **Reorganize**: Ensure the index (MEMORY.md) accurately reflects the current memory files

## Response Format
Respond with ONLY valid JSON (no markdown fencing):
{
  "actions": [
    {
      "action": "merge",
      "source_files": ["file1.md", "file2.md"],
      "merged_name": "combined name",
      "merged_description": "combined description",
      "merged_type": "user|feedback|project|reference",
      "merged_content": "combined markdown content"
    },
    {
      "action": "delete",
      "filename": "obsolete_file.md",
      "reason": "why this memory should be removed"
    },
    {
      "action": "update",
      "filename": "existing_file.md",
      "name": "updated name",
      "description": "updated description",
      "type": "user|feedback|project|reference",
      "content": "updated content"
    }
  ]
}

If no changes are needed, return: {"actions": []}"#.to_string()
}

/// Build the user message for consolidation with all current memories.
pub fn consolidation_user_message(memories: &[Memory]) -> String {
    let mut msg = String::from("## Current Memories\n\n");

    for mem in memories {
        msg.push_str(&format!(
            "### {} ({})\n**File**: {}\n**Type**: {}\n**Description**: {}\n\n{}\n\n---\n\n",
            mem.name,
            mem.path.file_name().unwrap_or_default().to_string_lossy(),
            mem.path.file_name().unwrap_or_default().to_string_lossy(),
            mem.memory_type,
            mem.description,
            mem.content
        ));
    }

    msg
}
