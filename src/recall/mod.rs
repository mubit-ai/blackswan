pub mod prompts;

use std::collections::HashSet;
use std::sync::Arc;

use crate::error::{MemoryError, Result};
use crate::llm::LlmProvider;
use crate::store::MemoryStore;
use crate::types::{Message, MessageRole, RecallResult};

/// Semantic memory recall: selects relevant memories for a query via LLM.
pub struct MemoryRecall<L: LlmProvider> {
    store: Arc<MemoryStore>,
    provider: Arc<L>,
    max_recall: usize,
}

impl<L: LlmProvider> MemoryRecall<L> {
    pub fn new(store: Arc<MemoryStore>, provider: Arc<L>, max_recall: usize) -> Self {
        Self {
            store,
            provider,
            max_recall,
        }
    }

    /// Select and return relevant memories for the given query.
    ///
    /// `already_surfaced` contains filenames of memories already shown in prior turns.
    /// `recently_used_tools` is passed to the selector to avoid redundant reference docs.
    ///
    /// On any LLM failure, returns an empty RecallResult (graceful degradation).
    pub async fn recall(
        &self,
        query: &str,
        already_surfaced: &HashSet<String>,
        recently_used_tools: &[String],
    ) -> Result<RecallResult> {
        let manifest = self.store.manifest()?;

        if manifest.entries.is_empty() {
            return Ok(RecallResult::default());
        }

        // Build manifest text, filtering already-surfaced
        let mut manifest_lines = Vec::new();
        let mut filtered = Vec::new();
        for entry in &manifest.entries {
            if already_surfaced.contains(&entry.filename) {
                filtered.push(entry.filename.clone());
                continue;
            }
            manifest_lines.push(format!(
                "- {} ({}) — {}",
                entry.filename, entry.title, entry.hook
            ));
        }

        if manifest_lines.is_empty() {
            return Ok(RecallResult {
                memories: vec![],
                filtered,
            });
        }

        let manifest_text = manifest_lines.join("\n");
        let system = prompts::recall_system_prompt();
        let user_msg = prompts::recall_user_message(query, &manifest_text, recently_used_tools);

        let messages = vec![Message {
            uuid: String::new(),
            role: MessageRole::User,
            content: user_msg,
        }];

        // Call LLM — on any failure, return empty
        let response = match self.provider.complete(messages, Some(system)).await {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(error = %e, "recall LLM call failed, returning empty");
                return Ok(RecallResult {
                    memories: vec![],
                    filtered,
                });
            }
        };

        // Parse JSON response
        let selected = match parse_recall_response(&response) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(error = %e, response = %response, "recall response parse failed, returning empty");
                return Ok(RecallResult {
                    memories: vec![],
                    filtered,
                });
            }
        };

        // Read selected memories (up to max_recall)
        let mut memories = Vec::new();
        for filename in selected.into_iter().take(self.max_recall) {
            match self.store.read(&filename) {
                Ok(memory) => memories.push(memory),
                Err(e) => {
                    tracing::warn!(filename = %filename, error = %e, "could not read selected memory");
                }
            }
        }

        Ok(RecallResult { memories, filtered })
    }
}

/// Parse the LLM's JSON response into a list of filenames.
fn parse_recall_response(response: &str) -> Result<Vec<String>> {
    // Try to extract JSON from the response (handle potential markdown fencing)
    let json_str = response
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();

    let value: serde_json::Value = serde_json::from_str(json_str).map_err(|e| {
        MemoryError::LlmResponseParse {
            message: format!("invalid JSON: {e}"),
        }
    })?;

    let arr = value
        .get("selected_memories")
        .and_then(|v| v.as_array())
        .ok_or_else(|| MemoryError::LlmResponseParse {
            message: "missing selected_memories array".into(),
        })?;

    let filenames: Vec<String> = arr
        .iter()
        .filter_map(|v| v.as_str().map(String::from))
        .collect();

    Ok(filenames)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_valid_response() {
        let json = r#"{"selected_memories": ["user_role.md", "feedback_prs.md"]}"#;
        let result = parse_recall_response(json).unwrap();
        assert_eq!(result, vec!["user_role.md", "feedback_prs.md"]);
    }

    #[test]
    fn parse_empty_response() {
        let json = r#"{"selected_memories": []}"#;
        let result = parse_recall_response(json).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn parse_fenced_response() {
        let json = "```json\n{\"selected_memories\": [\"a.md\"]}\n```";
        let result = parse_recall_response(json).unwrap();
        assert_eq!(result, vec!["a.md"]);
    }

    #[test]
    fn parse_invalid_json() {
        assert!(parse_recall_response("not json").is_err());
    }

    #[test]
    fn parse_missing_field() {
        let json = r#"{"other": "value"}"#;
        assert!(parse_recall_response(json).is_err());
    }
}
