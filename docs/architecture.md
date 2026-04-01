# Architecture

blackswan is built as a layered system with clear separation of concerns.

## System Overview

```
Your Agent Code
      |
      v
MemoryEngine<L>  ──────────────────────────────────────┐
      |                                                 |
      ├── recall()          → MemoryRecall<L>           |
      |                        ├── LlmProvider::complete |
      |                        └── MemoryStore::read     |
      |                                                 |
      ├── extract_background → Coalescer → Background Task
      |                                    └── MemoryExtractor<L>
      |                                         ├── LlmProvider::complete
      |                                         ├── MemoryStore::create/update
      |                                         └── ExtractionCursor
      |                                                 |
      ├── consolidate()     → ConsolidationGates        |
      |                        └── MemoryConsolidator<L>
      |                             ├── PidLock
      |                             ├── LlmProvider::complete
      |                             └── MemoryStore::*
      |                                                 |
      ├── create/update/delete_memory → WriteMutex      |
      |                                 └── MemoryStore  |
      |                                                 |
      └── is_enabled()     → EnableChain                |
                                                        |
MemoryStore ────────────────────────────────────────────┘
      ├── MemoryIndex    (MEMORY.md read/write)
      ├── Frontmatter    (YAML parse/serialize)
      └── FileOps        (scan, read, write, delete)
```

## Module Map

```
src/
  lib.rs                     Public re-exports
  engine.rs                  MemoryEngine<L> — orchestrator
  config.rs                  MemoryConfig + builder
  types.rs                   Core data types
  llm.rs                     LlmProvider trait
  error.rs                   MemoryError enum
  enable.rs                  Enable/disable priority chain
  staleness.rs               Age warnings + verification text
  store/
    mod.rs                   MemoryStore (CRUD coordinator)
    frontmatter.rs           YAML frontmatter parse/serialize
    index.rs                 MEMORY.md management + cap enforcement
    fileops.rs               File scanning, size checks, I/O
  extractor/
    mod.rs                   MemoryExtractor orchestration
    cursor.rs                Cursor persistence (.extraction-cursor)
    coalesce.rs              Single-slot stash + Notify
    prompts.rs               LLM system prompts for extraction
  recall/
    mod.rs                   MemoryRecall — semantic selection
    prompts.rs               LLM system prompts for recall
  consolidator/
    mod.rs                   MemoryConsolidator orchestration
    gates.rs                 Time/scan/session/lock gate sequence
    lock.rs                  PID-based filesystem lock
    prompts.rs               LLM system prompts for consolidation
```

## Storage Layout

All memory lives as flat files in a single directory:

```
<memory_dir>/
  MEMORY.md               Index file (loaded every turn)
  user_role.md             Individual memory files
  feedback_testing.md
  project_deadline.md
  .extraction-cursor       Last processed message UUID
  .consolidation-state     Session count + scan timestamps
  .consolidate-lock        PID lock for consolidation
```

Each `.md` file has YAML frontmatter:

```markdown
---
name: user prefers small PRs
description: corrected bundled PR approach — user wants atomic, reviewable changes
type: feedback
---

Don't bundle unrelated changes into a single PR.

**Why:** User was burned by a bundled PR that made review impossible and introduced a regression.
**How to apply:** When planning implementation, split into focused PRs by concern. One PR per logical change.
```

The `MEMORY.md` index is a flat list of one-line pointers:

```markdown
- [User prefers small PRs](feedback_prs.md) — corrected bundled PR approach
- [API migration deadline](project_api.md) — v3 API ships by 2026-04-15
```

## Concurrency Model

blackswan uses three concurrency mechanisms:

### Write Mutex (`tokio::sync::Mutex<()>`)

Shared between the background extraction task and direct CRUD methods. Ensures only one writer mutates memory files at a time. This is a tokio mutex because it's held across `.await` points.

### Extraction Coalescer (`std::sync::Mutex` + `tokio::sync::Notify`)

When `extract_background()` is called rapidly (e.g., multiple agent turns in quick succession), only the latest message batch is kept. The single-slot stash uses a `std::sync::Mutex` (held for nanoseconds, no await) and a `Notify` to wake the background task.

### PID Lock (filesystem)

The consolidation system uses a `.consolidate-lock` file containing the holder's PID. Stale lock detection checks both mtime and PID liveness. This is process-level mutual exclusion, not thread-level.

## Generics vs Dynamic Dispatch

`MemoryEngine<L>` is generic over `LlmProvider`. This gives zero-cost dispatch for the common case (one engine, one LLM). The `LlmProvider` trait uses RPITIT (`-> impl Future + Send`) instead of `async-trait`, which means:

- No heap allocation for the future
- No `async-trait` proc macro dependency
- Requires Rust 1.75+ (stable)
- The trait is not dyn-compatible (you can't use `Box<dyn LlmProvider>`)

If you need type erasure (e.g., switching LLM providers at runtime), wrap the engine behind your own trait object.

## Error Handling

Three tiers:

| Tier | Behavior | Examples |
|------|----------|----------|
| Hard errors | Propagated to caller via `Result` | File I/O failures, config errors, frontmatter parse errors |
| Soft errors | Logged, gracefully degraded | LLM failures during recall (returns empty), individual file parse errors during scan (skipped) |
| Background errors | Contained within the task | Extraction/consolidation failures are logged but don't crash the loop |

The library uses `tracing` for logging. Wire up your own subscriber (e.g., `tracing-subscriber`) to see warnings.
