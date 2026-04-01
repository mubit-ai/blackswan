# Memory Recall

Recall is the process of selecting relevant memories for a given query using LLM-based semantic matching.

## Basic Usage

```rust
let result = engine.recall("How should I handle error responses?", &[]).await?;

for memory in &result.memories {
    println!("{} ({}): {}", memory.name, memory.memory_type, memory.description);
    println!("{}\n", memory.content);
}
```

## How It Works

1. Load the MEMORY.md index
2. Filter out already-surfaced memories (deduplication across turns)
3. Build a manifest of available memories (filename, title, description)
4. Send the query + manifest to your LLM with a selector prompt
5. LLM returns JSON: `{"selected_memories": ["file1.md", "file2.md"]}`
6. Read and return the selected files (up to `max_recall`, default 5)

## Recently Used Tools

Pass tool names to avoid surfacing redundant reference documentation:

```rust
let tools = vec!["git".to_string(), "cargo".to_string()];
let result = engine.recall("How do I deploy?", &tools).await?;
```

The LLM selector is instructed to skip reference memories for tools already in use, but still surface warnings or gotchas about those tools.

## Deduplication

blackswan tracks which memories have been surfaced during a session. On each recall, already-surfaced memories are excluded from the candidate list so the LLM spends its 5-slot budget on fresh candidates.

The surfaced set is cleared when:
- `record_session_end()` is called
- The engine is restarted

```rust
// Surfaced set for this session
println!("Filtered out: {:?}", result.filtered);
```

## Graceful Degradation

Recall never fails the caller. If anything goes wrong:

| Failure | Behavior |
|---------|----------|
| LLM call fails (network, timeout, etc.) | Returns empty `RecallResult` |
| LLM returns invalid JSON | Returns empty `RecallResult` |
| LLM selects a file that doesn't exist | Skipped, other selections still returned |
| No memories exist | Returns empty `RecallResult` |
| Memory system disabled | Returns empty `RecallResult` |

All failures are logged via `tracing::warn`.

## RecallResult

```rust
pub struct RecallResult {
    /// The selected memories with full content.
    pub memories: Vec<Memory>,
    /// Filenames that were filtered out (already surfaced this session).
    pub filtered: Vec<String>,
}
```

## Staleness

Memories returned by recall may be stale. Use the staleness utilities to warn users:

```rust
use blackswan::Staleness;

for memory in &result.memories {
    if let Some(modified) = memory.modified {
        let staleness = blackswan::staleness::compute_staleness(modified);
        if let Some(warning) = blackswan::staleness::staleness_warning(staleness) {
            println!("Warning: {}", warning);
        }
    }
}
```

Staleness levels:
- **0-1 days**: Fresh (no warning)
- **2+ days**: Warning with age — "verify against current state before asserting as fact"

## Verification

Memories that reference specific files, functions, or APIs may be outdated. Before acting on recalled memories:

- If the memory names a file path: check the file exists
- If the memory names a function or API: search for it
- If the user is about to act on your recommendation: verify first

This verification is advisory — blackswan does not enforce it at runtime.
