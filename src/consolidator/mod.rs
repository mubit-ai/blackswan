pub mod gates;
pub mod lock;
pub mod prompts;

use std::sync::Arc;

use crate::error::{MemoryError, Result};
use crate::llm::LlmProvider;
use crate::store::MemoryStore;
use crate::types::{Memory, MemoryType, Message, MessageRole};

use self::gates::{ConsolidationGates, GateResult};

/// Orchestrates memory consolidation ("dream" system).
///
/// Reviews, deduplicates, and reorganizes memories via an LLM agent.
pub struct MemoryConsolidator<L: LlmProvider> {
    store: Arc<MemoryStore>,
    provider: Arc<L>,
    gates: ConsolidationGates,
    #[allow(dead_code)]
    max_turns: usize,
}

impl<L: LlmProvider> MemoryConsolidator<L> {
    pub fn new(store: Arc<MemoryStore>, provider: Arc<L>, config: Arc<crate::config::MemoryConfig>) -> Self {
        let max_turns = config.consolidation_max_turns;
        let gates = ConsolidationGates::new(config);
        Self {
            store,
            provider,
            gates,
            max_turns,
        }
    }

    pub fn gates(&self) -> &ConsolidationGates {
        &self.gates
    }

    /// Attempt to run consolidation. Respects all gates.
    /// Returns Ok(true) if consolidation ran, Ok(false) if gated.
    pub async fn run(&self) -> Result<bool> {
        // Evaluate gates
        match self.gates.evaluate() {
            GateResult::Block { reason } => {
                tracing::debug!(reason = %reason, "consolidation gated");
                return Ok(false);
            }
            GateResult::Pass => {}
        }

        // Try to acquire lock
        let lock = self.gates.lock();
        let _guard = lock.try_acquire(
            std::time::Duration::from_secs(3600), // 1h stale timeout
        )?;

        // Run the consolidation
        let result = self.do_consolidation().await;

        match &result {
            Ok(()) => {
                // Mark successful consolidation
                lock.touch()?;
                self.gates.reset_sessions()?;
                tracing::info!("consolidation completed successfully");
            }
            Err(e) => {
                tracing::warn!(error = %e, "consolidation failed");
                // Guard will drop and release the lock
            }
        }

        result.map(|()| true)
    }

    async fn do_consolidation(&self) -> Result<()> {
        let memories = self.store.scan_all()?;

        if memories.is_empty() {
            return Ok(());
        }

        let system = prompts::consolidation_system_prompt();
        let user_msg = prompts::consolidation_user_message(&memories);

        let messages = vec![Message {
            uuid: String::new(),
            role: MessageRole::User,
            content: user_msg,
        }];

        let response = self.provider.complete(messages, Some(system)).await?;
        self.execute_actions(&response)?;

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
                message: format!("consolidation response is not valid JSON: {e}"),
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
                "merge" => {
                    if let Err(e) = self.handle_merge(action) {
                        tracing::warn!(error = %e, "consolidation merge failed");
                    }
                }
                "delete" => {
                    let filename = action
                        .get("filename")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    if let Err(e) = self.store.delete(filename) {
                        tracing::warn!(error = %e, filename = %filename, "consolidation delete failed");
                    }
                }
                "update" => {
                    let filename = action
                        .get("filename")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let memory = parse_action_memory(action)?;
                    if let Err(e) = self.store.update(filename, &memory) {
                        tracing::warn!(error = %e, filename = %filename, "consolidation update failed");
                    }
                }
                other => {
                    tracing::warn!(action = %other, "unknown consolidation action, skipping");
                }
            }
        }

        Ok(())
    }

    fn handle_merge(&self, action: &serde_json::Value) -> Result<()> {
        // Delete source files
        let sources = action
            .get("source_files")
            .and_then(|v| v.as_array())
            .ok_or_else(|| MemoryError::LlmResponseParse {
                message: "merge action missing source_files".into(),
            })?;

        for source in sources {
            if let Some(filename) = source.as_str() {
                let _ = self.store.delete(filename);
            }
        }

        // Create merged memory
        let name = action
            .get("merged_name")
            .and_then(|v| v.as_str())
            .unwrap_or("merged memory")
            .to_string();
        let description = action
            .get("merged_description")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let type_str = action
            .get("merged_type")
            .and_then(|v| v.as_str())
            .unwrap_or("user");
        let content = action
            .get("merged_content")
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

        let memory = Memory {
            name,
            description,
            memory_type,
            content,
            path: std::path::PathBuf::new(),
            modified: None,
        };

        self.store.create(&memory)
    }
}

fn parse_action_memory(action: &serde_json::Value) -> Result<Memory> {
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
