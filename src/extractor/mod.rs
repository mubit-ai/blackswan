pub mod coalesce;
pub mod cursor;
pub mod prompts;

use std::sync::Arc;

use crate::error::{MemoryError, Result};
use crate::llm::LlmProvider;
use crate::store::MemoryStore;
use crate::types::{Memory, MemoryType, Message, MessageRole};

use self::cursor::ExtractionCursor;

/// Orchestrates memory extraction from conversation messages.
pub struct MemoryExtractor<L: LlmProvider> {
    store: Arc<MemoryStore>,
    provider: Arc<L>,
    cursor: ExtractionCursor,
    #[allow(dead_code)]
    max_turns: usize,
}

impl<L: LlmProvider> MemoryExtractor<L> {
    pub fn new(store: Arc<MemoryStore>, provider: Arc<L>, max_turns: usize) -> Self {
        let cursor = ExtractionCursor::new(store.memory_dir());
        Self {
            store,
            provider,
            cursor,
            max_turns,
        }
    }

    /// Run extraction on the given messages.
    ///
    /// Filters messages to only those after the cursor, calls the LLM,
    /// and executes the resulting memory operations.
    pub async fn run(&self, messages: Vec<Message>) -> Result<()> {
        if messages.is_empty() {
            return Ok(());
        }

        // Filter to messages after cursor
        let cursor_uuid = self.cursor.load()?;
        let messages_to_process = if let Some(ref cursor) = cursor_uuid {
            let pos = messages.iter().position(|m| m.uuid == *cursor);
            match pos {
                Some(idx) => messages[idx + 1..].to_vec(),
                None => messages, // cursor not found, process all
            }
        } else {
            messages
        };

        if messages_to_process.is_empty() {
            return Ok(());
        }

        let last_uuid = messages_to_process.last().unwrap().uuid.clone();

        // Build existing memory manifest for context
        let existing = self.store.scan_all()?;

        // Build the LLM prompt
        let system = prompts::extraction_system_prompt();
        let user_msg = prompts::extraction_user_message(&messages_to_process, &existing);

        let llm_messages = vec![Message {
            uuid: String::new(),
            role: MessageRole::User,
            content: user_msg,
        }];

        // Call LLM (single turn for now; multi-turn loop for complex extractions)
        let response = self.provider.complete(llm_messages, Some(system)).await?;

        // Parse and execute actions
        self.execute_actions(&response)?;

        // Advance cursor only on success
        self.cursor.save(&last_uuid)?;

        Ok(())
    }

    fn execute_actions(&self, response: &str) -> Result<()> {
        let json_str = response
            .trim()
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim();

        let value: serde_json::Value =
            serde_json::from_str(json_str).map_err(|e| MemoryError::LlmResponseParse {
                message: format!("extraction response is not valid JSON: {e}"),
            })?;

        let actions = value
            .get("actions")
            .and_then(|v| v.as_array())
            .ok_or_else(|| MemoryError::LlmResponseParse {
                message: "missing actions array".into(),
            })?;

        for action in actions {
            let action_type = action
                .get("action")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            match action_type {
                "create" => {
                    let memory = parse_memory_from_action(action)?;
                    if let Err(e) = self.store.create(&memory) {
                        tracing::warn!(error = %e, name = %memory.name, "extraction create failed");
                    }
                }
                "update" => {
                    let filename = action
                        .get("filename")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let memory = parse_memory_from_action(action)?;
                    if let Err(e) = self.store.update(filename, &memory) {
                        tracing::warn!(error = %e, filename = %filename, "extraction update failed");
                    }
                }
                "delete" => {
                    let filename = action
                        .get("filename")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    if let Err(e) = self.store.delete(filename) {
                        tracing::warn!(error = %e, filename = %filename, "extraction delete failed");
                    }
                }
                other => {
                    tracing::warn!(action = %other, "unknown extraction action, skipping");
                }
            }
        }

        Ok(())
    }
}

fn parse_memory_from_action(action: &serde_json::Value) -> Result<Memory> {
    let name = action
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("unnamed")
        .to_string();
    let description = action
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let type_str = action
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("user");
    let content = action
        .get("content")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let memory_type = match type_str {
        "user" => MemoryType::User,
        "feedback" => MemoryType::Feedback,
        "project" => MemoryType::Project,
        "reference" => MemoryType::Reference,
        _ => MemoryType::User,
    };

    Ok(Memory {
        name,
        description,
        memory_type,
        content,
        path: std::path::PathBuf::new(),
        modified: None,
    })
}
