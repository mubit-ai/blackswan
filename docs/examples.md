# Examples

## Minimal Agent with Memory

A complete working example of an agent loop with memory extraction and recall.

```rust
use blackswan::*;
use std::future::Future;

// 1. Implement LlmProvider for your model
struct EchoLlm;

impl LlmProvider for EchoLlm {
    fn complete(
        &self,
        messages: Vec<Message>,
        _system: Option<String>,
    ) -> impl Future<Output = Result<String>> + Send + '_ {
        async move {
            // Replace this with actual LLM calls.
            // For demo, return empty actions / empty selection.
            Ok(r#"{"actions": []}"#.to_string())
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let config = MemoryConfig::builder("./my-agent-memories").build()?;
    let engine = MemoryEngine::new(config, EchoLlm).await?;

    // Agent loop
    let mut conversation: Vec<Message> = Vec::new();
    let mut turn = 0;

    loop {
        // (In a real agent, get user input here)
        let user_input = "Tell me about error handling in Rust";
        conversation.push(Message {
            uuid: format!("msg-{turn}"),
            role: MessageRole::User,
            content: user_input.into(),
        });

        // Recall relevant memories before generating response
        let recalled = engine.recall(user_input, &[]).await?;
        for mem in &recalled.memories {
            println!("  Recalled: [{}] {}", mem.memory_type, mem.name);
        }

        // (In a real agent, generate response using LLM + recalled memories)
        let assistant_response = "Here's how error handling works...";
        turn += 1;
        conversation.push(Message {
            uuid: format!("msg-{turn}"),
            role: MessageRole::Assistant,
            content: assistant_response.into(),
        });
        turn += 1;

        // Extract memories from the conversation
        engine.extract_background(conversation.clone());

        break; // demo: single turn
    }

    // End of session
    engine.record_session_end().await;
    engine.consolidate_background().await;
    engine.shutdown().await;
    Ok(())
}
```

## Using with Anthropic API

```rust
use blackswan::*;
use std::future::Future;

struct AnthropicLlm {
    api_key: String,
    client: reqwest::Client,
}

impl LlmProvider for AnthropicLlm {
    fn complete(
        &self,
        messages: Vec<Message>,
        system: Option<String>,
    ) -> impl Future<Output = Result<String>> + Send + '_ {
        async move {
            let api_messages: Vec<serde_json::Value> = messages
                .iter()
                .map(|m| serde_json::json!({
                    "role": match m.role {
                        MessageRole::User => "user",
                        MessageRole::Assistant => "assistant",
                        MessageRole::System => "user",
                    },
                    "content": m.content,
                }))
                .collect();

            let mut body = serde_json::json!({
                "model": "claude-haiku-4-5-20251001",
                "max_tokens": 4096,
                "messages": api_messages,
            });

            if let Some(sys) = system {
                body["system"] = serde_json::json!(sys);
            }

            let resp = self.client
                .post("https://api.anthropic.com/v1/messages")
                .header("x-api-key", &self.api_key)
                .header("anthropic-version", "2023-06-01")
                .header("content-type", "application/json")
                .json(&body)
                .send()
                .await
                .map_err(|e| MemoryError::LlmError { message: e.to_string() })?;

            let json: serde_json::Value = resp.json().await
                .map_err(|e| MemoryError::LlmError { message: e.to_string() })?;

            json["content"][0]["text"]
                .as_str()
                .map(String::from)
                .ok_or_else(|| MemoryError::LlmResponseParse {
                    message: "no text in response".into(),
                })
        }
    }
}
```

## Using with OpenAI API

```rust
struct OpenAiLlm {
    api_key: String,
    client: reqwest::Client,
}

impl LlmProvider for OpenAiLlm {
    fn complete(
        &self,
        messages: Vec<Message>,
        system: Option<String>,
    ) -> impl Future<Output = Result<String>> + Send + '_ {
        async move {
            let mut api_messages: Vec<serde_json::Value> = Vec::new();

            if let Some(sys) = system {
                api_messages.push(serde_json::json!({
                    "role": "system",
                    "content": sys,
                }));
            }

            for m in &messages {
                api_messages.push(serde_json::json!({
                    "role": match m.role {
                        MessageRole::User => "user",
                        MessageRole::Assistant => "assistant",
                        MessageRole::System => "system",
                    },
                    "content": m.content,
                }));
            }

            let body = serde_json::json!({
                "model": "gpt-4o-mini",
                "messages": api_messages,
            });

            let resp = self.client
                .post("https://api.openai.com/v1/chat/completions")
                .header("Authorization", format!("Bearer {}", self.api_key))
                .json(&body)
                .send()
                .await
                .map_err(|e| MemoryError::LlmError { message: e.to_string() })?;

            let json: serde_json::Value = resp.json().await
                .map_err(|e| MemoryError::LlmError { message: e.to_string() })?;

            json["choices"][0]["message"]["content"]
                .as_str()
                .map(String::from)
                .ok_or_else(|| MemoryError::LlmResponseParse {
                    message: "no content in response".into(),
                })
        }
    }
}
```

## Manual Memory Management

```rust
// Create memories directly (no LLM involved)
engine.create_memory(&Memory {
    name: "team uses Linear for bugs".into(),
    description: "bug tracking is in Linear project INGEST".into(),
    memory_type: MemoryType::Reference,
    content: "Pipeline bugs tracked in Linear project \"INGEST\".\nURL: https://linear.app/company/project/ingest".into(),
    path: Default::default(),
    modified: None,
}).await?;

// Read it back
let mem = engine.read_memory("team_uses_linear_for_bugs.md")?;
println!("{}", mem.content);

// Update it
engine.update_memory("team_uses_linear_for_bugs.md", &Memory {
    name: "team uses Linear for bugs".into(),
    description: "bug tracking moved to Jira project PIPE".into(),
    memory_type: MemoryType::Reference,
    content: "Pipeline bugs now tracked in Jira project \"PIPE\" (migrated from Linear 2026-04).".into(),
    path: Default::default(),
    modified: None,
}).await?;

// Delete it
engine.delete_memory("team_uses_linear_for_bugs.md").await?;

// Browse the index
let manifest = engine.manifest()?;
for entry in &manifest.entries {
    println!("- [{}]({}) — {}", entry.title, entry.filename, entry.hook);
}
```

## Custom Consolidation Schedule

```rust
use std::time::Duration;

let config = MemoryConfig::builder("./memories")
    // Run consolidation after every 2 sessions instead of 5
    .consolidation_session_gate(2)
    // With only 4 hours between runs instead of 24
    .consolidation_cooldown(Duration::from_secs(4 * 3600))
    .build()?;

let engine = MemoryEngine::new(config, my_llm).await?;

// After each session
engine.record_session_end().await;

// This will now trigger consolidation more frequently
engine.consolidate_background().await;
```
