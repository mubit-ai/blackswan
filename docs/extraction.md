# Memory Extraction

Extraction is the process of analyzing conversation messages and automatically creating, updating, or deleting memories.

## How It Works

After each agent response, you feed the conversation messages to the engine:

```rust
engine.extract_background(messages);
```

The extraction pipeline:

1. Messages are pushed into a **single-slot stash** (coalescing)
2. The background task wakes up and acquires the write mutex
3. The **cursor** filters to only unprocessed messages
4. Your LLM analyzes the messages against the existing memory manifest
5. The LLM returns create/update/delete actions
6. Actions are executed against the store
7. The cursor advances to the last processed message

## Background vs Synchronous

### Background (recommended for production)

```rust
engine.extract_background(messages);
// Returns immediately. Extraction runs on a background tokio task.
```

Features:
- Non-blocking
- Coalesces rapid-fire calls (only the latest batch is kept)
- Automatically retries on next push if extraction fails

### Synchronous (for testing)

```rust
engine.extract(messages).await?;
// Blocks until extraction completes. Returns errors directly.
```

Use this in tests where you need deterministic behavior.

## Coalescing

If `extract_background()` is called multiple times before the background task processes:

```
Call 1: [msg-1, msg-2]     → stashed
Call 2: [msg-1, msg-2, msg-3]  → replaces stash
Call 3: [msg-1, msg-2, msg-3, msg-4]  → replaces stash
                                        ↑ only this batch is processed
```

This is a deliberate tradeoff: intermediate contexts are lost for simplicity. The assumption is that later message batches are supersets of earlier ones.

## Cursor Management

The extractor tracks the UUID of the last processed message in `.extraction-cursor`. On each run:

1. Load the cursor
2. Find the cursor UUID in the message list
3. Process only messages after the cursor
4. On success, save the new cursor (last message UUID)

If the cursor UUID isn't found in the message list (e.g., after a context window compaction), all messages are processed.

If extraction fails, the cursor does not advance — those messages will be reconsidered next time.

## Mutual Exclusion

The extraction background task shares a `tokio::sync::Mutex<()>` (the write mutex) with direct CRUD operations (`create_memory`, `update_memory`, `delete_memory`). This ensures:

- The main agent's manual memory writes don't race with background extraction
- Only one writer modifies memory files at a time

The write mutex does NOT prevent concurrent reads (recall, manifest).

## What the LLM Sees

The extraction prompt includes:

1. **System prompt**: Instructions on the 4 memory types, what to save vs. not save, and the expected JSON response format
2. **Conversation messages**: The messages to analyze, with roles
3. **Existing memory manifest**: Type, filename, name, and description of each existing memory (so the LLM can decide to update rather than create duplicates)

The LLM responds with a JSON action list:

```json
{
  "actions": [
    {
      "action": "create",
      "name": "user is backend engineer",
      "description": "user specializes in Rust backend development",
      "type": "user",
      "content": "The user is a backend engineer who primarily works in Rust."
    },
    {
      "action": "update",
      "filename": "project_deadline.md",
      "name": "project deadline",
      "description": "API v3 deadline extended to April 15",
      "type": "project",
      "content": "API v3 deadline: 2026-04-15 (extended from March 31)"
    },
    {
      "action": "delete",
      "filename": "obsolete_note.md"
    }
  ]
}
```

## Controlling Extraction Frequency

By default, extraction runs on every turn (`extraction_turn_interval = 1`). For high-frequency agents:

```rust
let config = MemoryConfig::builder("./memories")
    .extraction_turn_interval(3) // every 3rd turn
    .build()?;
```

## Error Handling

- If the LLM call fails: logged as a warning, extraction skipped, cursor does not advance
- If JSON parsing fails: logged, extraction skipped
- If individual create/update/delete fails: logged, other actions still execute
- Background task never crashes — it catches all errors and continues the loop
