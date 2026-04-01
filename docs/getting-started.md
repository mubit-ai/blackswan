# Getting Started

## Installation

Add `blackswan` to your `Cargo.toml`:

```toml
[dependencies]
blackswan = "0.1"
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
```

## Quick Start

Three steps: implement the LLM trait, configure the engine, use it.

### 1. Implement `LlmProvider`

blackswan is bring-your-own-LLM. You provide a single trait implementation that the library uses for all LLM calls (extraction, recall, consolidation).

```rust
use blackswan::{LlmProvider, MemoryError, Message};
use std::future::Future;

struct MyLlm {
    api_key: String,
}

impl LlmProvider for MyLlm {
    fn complete(
        &self,
        messages: Vec<Message>,
        system: Option<String>,
    ) -> impl Future<Output = Result<String, MemoryError>> + Send + '_ {
        async move {
            // Forward to your LLM of choice (OpenAI, Anthropic, Ollama, etc.)
            // The library sends pre-formatted prompts — just relay and return the text.
            let response = call_my_api(&self.api_key, messages, system).await
                .map_err(|e| MemoryError::LlmError { message: e.to_string() })?;
            Ok(response)
        }
    }
}
```

### 2. Create the Engine

```rust
use blackswan::{MemoryEngine, MemoryConfig};

#[tokio::main]
async fn main() -> blackswan::Result<()> {
    let config = MemoryConfig::builder("./memories")
        .build()?;

    let llm = MyLlm { api_key: "sk-...".into() };
    let engine = MemoryEngine::new(config, llm).await?;

    // The engine is ready. A background extraction loop is already running.
    Ok(())
}
```

### 3. Use It

```rust
// After each agent response, feed the conversation for extraction
let messages = vec![
    Message {
        uuid: "msg-1".into(),
        role: MessageRole::User,
        content: "I'm a backend engineer working in Rust.".into(),
    },
    Message {
        uuid: "msg-2".into(),
        role: MessageRole::Assistant,
        content: "Got it! I'll tailor my responses accordingly.".into(),
    },
];

// Background extraction (non-blocking, coalesces rapid-fire calls)
engine.extract_background(messages);

// Recall relevant memories for a new query
let result = engine.recall("How should I structure this module?", &[]).await?;
for memory in &result.memories {
    println!("[{}] {}: {}", memory.memory_type, memory.name, memory.description);
}

// Direct CRUD for manual memory management
use blackswan::{Memory, MemoryType};

engine.create_memory(&Memory {
    name: "project deadline".into(),
    description: "API v3 ships by 2026-04-15".into(),
    memory_type: MemoryType::Project,
    content: "API v3 deadline: 2026-04-15\n\n**Why:** contractual obligation with partner\n**How to apply:** prioritize API work over refactoring".into(),
    path: Default::default(),
    modified: None,
}).await?;

// At end of session, record it (drives consolidation gating)
engine.record_session_end().await;

// Clean shutdown
engine.shutdown().await;
```

## What Happens Under the Hood

When you call `MemoryEngine::new`:
1. The memory directory is created if it doesn't exist
2. A background tokio task starts, listening for extraction requests
3. The MEMORY.md index is loaded (or created empty)

When you call `extract_background`:
1. Messages are pushed into a single-slot stash (coalescing rapid-fire calls)
2. The background task wakes up, acquires the write mutex, and calls your LLM
3. The LLM decides what to create/update/delete, and the store executes those operations
4. A cursor file tracks the last processed message UUID

When you call `recall`:
1. The MEMORY.md index is read
2. Already-surfaced memories are filtered out
3. Your LLM picks up to 5 relevant memories from the manifest
4. Those files are read and returned with their full content

## Next Steps

- [Architecture Guide](./architecture.md) for the full system design
- [Memory Types](./memory-types.md) for what to store and how
- [Configuration Reference](./configuration.md) for all tuning knobs
- [Consolidation](./consolidation.md) for the "dream" garbage collector
