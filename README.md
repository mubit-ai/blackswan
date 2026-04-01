# blackswan

A persistent memory system for AI agents. Plug-and-play memory engine with flat-file storage, background extraction, semantic recall, and automatic consolidation.

Built in Rust with native bindings for Python and TypeScript/Node.js.

## Features

- **4-type memory taxonomy**: user, feedback, project, reference
- **Background extraction**: automatically extracts memories from conversations via your LLM
- **Semantic recall**: LLM-powered selection of relevant memories for each query
- **Consolidation ("dream" system)**: background deduplication and cleanup
- **Flat-file storage**: human-readable markdown with YAML frontmatter
- **Bring your own LLM**: works with any model (Claude, GPT, Ollama, etc.)

## Installation

### Rust

```toml
# Cargo.toml
[dependencies]
blackswan = { git = "https://github.com/mubit-ai/blackswan" }
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
```

### Python

Requires Rust toolchain installed.

```bash
pip install "blackswan @ git+https://github.com/mubit-ai/blackswan#subdirectory=bindings/python"
```

### TypeScript / Node.js

Requires Rust toolchain installed.

```bash
npm install github:mubit-ai/blackswan
```

## Quick Start

### Python

```python
import asyncio
from blackswan import MemoryEngine, MemoryConfig, Memory

async def my_llm(messages, system):
    """Your LLM call — forward messages and system prompt, return text."""
    # Replace with your actual LLM API call
    return '{"selected_memories": []}'

async def main():
    config = MemoryConfig("./memories")
    engine = await MemoryEngine.create(config, my_llm)

    # Create a memory directly
    await engine.create_memory(Memory(
        name="user is backend engineer",
        description="user specializes in Rust backend development",
        memory_type="user",
        content="The user is a backend engineer who primarily works in Rust.",
    ))

    # Recall relevant memories for a query
    result = await engine.recall("How should I structure this module?")
    for mem in result.memories:
        print(f"[{mem.memory_type}] {mem.name}: {mem.description}")

    # Extract memories from a conversation (background, non-blocking)
    engine.extract_background([
        {"uuid": "msg-1", "role": "user", "content": "I prefer small PRs"},
        {"uuid": "msg-2", "role": "assistant", "content": "Noted!"},
    ])

    await engine.shutdown()

asyncio.run(main())
```

### TypeScript / Node.js

```typescript
import { MemoryEngine, MemoryConfig } from 'blackswan'

async function myLlm(messages: any[], system: string | null): Promise<string> {
  // Replace with your actual LLM API call
  return JSON.stringify({ selected_memories: [] })
}

const config = new MemoryConfig('./memories')
const engine = await MemoryEngine.create(config, myLlm)

// Create a memory
await engine.createMemory({
  name: 'user is backend engineer',
  description: 'user specializes in Rust backend development',
  memoryType: 'user',
  content: 'The user is a backend engineer who primarily works in Rust.',
})

// Recall
const result = await engine.recall('How should I structure this module?')
for (const mem of result.memories) {
  console.log(`[${mem.memoryType}] ${mem.name}: ${mem.description}`)
}

await engine.shutdown()
```

### Rust

```rust
use blackswan::{MemoryEngine, MemoryConfig, LlmProvider, Memory, MemoryType, Message, MemoryError};

struct MyLlm;

impl LlmProvider for MyLlm {
    fn complete(
        &self,
        messages: Vec<Message>,
        system: Option<String>,
    ) -> impl std::future::Future<Output = Result<String, MemoryError>> + Send + '_ {
        async move {
            // Replace with your actual LLM call
            Ok(r#"{"selected_memories": []}"#.to_string())
        }
    }
}

#[tokio::main]
async fn main() -> blackswan::Result<()> {
    let config = MemoryConfig::builder("./memories").build()?;
    let engine = MemoryEngine::new(config, MyLlm).await?;

    engine.create_memory(&Memory {
        name: "user is backend engineer".into(),
        description: "user specializes in Rust".into(),
        memory_type: MemoryType::User,
        content: "The user is a backend engineer.".into(),
        path: Default::default(),
        modified: None,
    }).await?;

    let result = engine.recall("How should I structure this?", &[]).await?;
    for mem in &result.memories {
        println!("[{}] {}", mem.memory_type, mem.name);
    }

    engine.shutdown().await;
    Ok(())
}
```

## Memory Types

| Type | What to Store | Structure |
|------|--------------|-----------|
| `user` | Role, goals, preferences, expertise | Free-form |
| `feedback` | Corrections, confirmed approaches | Rule / Why / How to apply |
| `project` | Initiatives, deadlines, decisions | Fact / Why / How to apply |
| `reference` | Pointers to external systems | System + URL + purpose |

## Storage Format

Each memory is a markdown file with YAML frontmatter:

```markdown
---
name: user prefers small PRs
description: corrected bundled PR approach
type: feedback
---

Don't bundle unrelated changes into a single PR.

**Why:** Prior incident where a bundled PR masked a regression.
**How to apply:** Split PRs by concern. One PR per logical change.
```

An index file (`MEMORY.md`) provides a flat list of one-line pointers:

```markdown
- [User prefers small PRs](feedback_prs.md) — corrected bundled PR approach
- [API migration deadline](project_api.md) — v3 ships by 2026-04-15
```

## Architecture

```
MemoryEngine
  ├── recall()            → LLM selects relevant memories
  ├── extract_background  → Background task extracts from conversation
  ├── create/update/delete → Direct CRUD with write mutex
  ├── consolidate()       → Gated background dedup/cleanup
  └── is_enabled()        → Enable/disable chain
```

See [docs/](./docs/) for full architecture, configuration reference, and detailed guides.

## Configuration

```rust
// Rust
MemoryConfig::builder("./memories")
    .max_recall(5)
    .consolidation_session_gate(3)
    .build()?;
```

```python
# Python
MemoryConfig("./memories", max_recall=5, consolidation_session_gate=3)
```

```typescript
// TypeScript
new MemoryConfig({ memoryDir: './memories', maxRecall: 5 })
```

All parameters have sensible defaults. See [docs/configuration.md](./docs/configuration.md) for the full reference.

## Documentation

- [Getting Started](./docs/getting-started.md)
- [Architecture](./docs/architecture.md)
- [Memory Types](./docs/memory-types.md)
- [Configuration Reference](./docs/configuration.md)
- [Extraction](./docs/extraction.md)
- [Recall](./docs/recall.md)
- [Consolidation](./docs/consolidation.md)
- [API Reference](./docs/api-reference.md)
- [Examples](./docs/examples.md)

## License

Apache-2.0. See [LICENSE](./LICENSE).
