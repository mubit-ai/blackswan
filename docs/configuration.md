# Configuration Reference

All configuration is done through `MemoryConfig`, constructed via the builder pattern.

## Builder Usage

```rust
use blackswan::MemoryConfig;
use std::time::Duration;

let config = MemoryConfig::builder("/path/to/memories")
    .max_index_lines(200)
    .max_recall(5)
    .consolidation_cooldown(Duration::from_secs(24 * 3600))
    .build()?;
```

The only required parameter is the memory directory path. Everything else has sensible defaults.

## Parameters

### Storage Limits

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `max_index_lines` | `usize` | 200 | Maximum lines in MEMORY.md. Truncates with warning when exceeded. |
| `max_index_bytes` | `usize` | 25,600 (25KB) | Maximum bytes in MEMORY.md. Truncates at last complete line. |
| `max_scan_files` | `usize` | 200 | Maximum memory files to scan. Sorted by mtime descending; extras are invisible. |
| `large_file_warning_bytes` | `u64` | 40,960 (40KB) | Log a warning when a memory file exceeds this size. |

### Extraction

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `extraction_turn_interval` | `usize` | 1 | Run extraction every N turns. Set to 2+ to reduce LLM calls. |
| `extraction_max_turns` | `usize` | 5 | Maximum LLM conversation turns per extraction run. |

### Consolidation

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `consolidation_cooldown` | `Duration` | 24 hours | Minimum time between consolidation runs. |
| `consolidation_scan_throttle` | `Duration` | 10 minutes | Don't re-scan session list more frequently than this. |
| `consolidation_session_gate` | `usize` | 5 | Minimum sessions since last consolidation before eligible. |
| `consolidation_lock_timeout` | `Duration` | 60 minutes | Consider a lock stale if mtime exceeds this and PID is dead. |
| `consolidation_max_turns` | `usize` | 30 | Maximum LLM turns per consolidation run. |

### Recall

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `max_recall` | `usize` | 5 | Maximum memories returned per recall query. |

### Enable/Disable

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `enabled_override` | `Option<bool>` | `None` | Force enable/disable. `None` uses the chain logic. |
| `bare_mode` | `bool` | `false` | Disables memory (e.g., for minimal/headless agents). |
| `remote_mode` | `bool` | `false` | Disables memory (no persistent storage available). |

## Enable/Disable Chain

The system evaluates this priority chain (first match wins):

1. **`AGENT_MEMORY_ENABLED=0`** environment variable → OFF
2. **`bare_mode = true`** → OFF
3. **`remote_mode = true`** → OFF
4. **`enabled_override`** in config → respect the setting
5. **Default** → ON

```rust
// Disable via environment
std::env::set_var("AGENT_MEMORY_ENABLED", "0");

// Disable via config
let config = MemoryConfig::builder("./memories")
    .enabled_override(Some(false))
    .build()?;

// Check at runtime
if engine.is_enabled() {
    // memory is active
}
```

## Context Window Budget

Memory consumes context window space in the host agent. Budget carefully:

| Component | Token Estimate | Frequency |
|-----------|---------------|-----------|
| Memory instructions (types, save/recall rules) | ~500-1,000 | Every turn (static) |
| MEMORY.md content | ~5-6K (at 25KB cap) | Every turn (cacheable) |
| Surfaced memories (via recall) | ~4K (5 files x ~800 tokens) | Per-turn, on-demand |

**Total steady-state overhead: 10-15% of context window.** This is non-compressible — it's injected fresh every turn.

## Tuning Tips

**High-frequency agents** (many short turns): Increase `extraction_turn_interval` to 2-3 to reduce LLM calls.

**Long-running agents** (few sessions per day): Decrease `consolidation_session_gate` to 2-3 so consolidation runs more often.

**Memory-constrained agents** (small context window): Decrease `max_recall` to 3 and `max_index_lines` to 100.

**Multi-process deployments**: The PID lock handles cross-process consolidation. Each process can run its own engine pointing at the same directory. Writes are not coordinated across processes — use a shared filesystem lock if needed.
