# API Reference

Complete reference for all public types and methods in blackswan.

## MemoryEngine\<L: LlmProvider\>

The main entry point. Generic over the LLM provider.

### Construction

```rust
pub async fn new(config: MemoryConfig, provider: L) -> Result<Self>
```

Creates the engine, ensures the memory directory exists, and spawns the background extraction loop.

### Recall

```rust
pub async fn recall(&self, query: &str, recently_used_tools: &[String]) -> Result<RecallResult>
```

Select and return relevant memories for the query. Returns up to `max_recall` memories. On LLM failure, returns empty result.

```rust
pub fn manifest(&self) -> Result<MemoryManifest>
```

Return the full MEMORY.md index.

```rust
pub fn read_memory(&self, name: &str) -> Result<Memory>
```

Read a single memory by filename (e.g., `"user_role.md"`) or name (e.g., `"user_role"`).

### Extraction

```rust
pub fn extract_background(&self, messages: Vec<Message>)
```

Spawn background extraction. Non-blocking, coalesces rapid-fire calls.

```rust
pub async fn extract(&self, messages: Vec<Message>) -> Result<()>
```

Run extraction synchronously. Blocks until complete.

### Direct CRUD

```rust
pub async fn create_memory(&self, memory: &Memory) -> Result<()>
pub async fn update_memory(&self, name: &str, memory: &Memory) -> Result<()>
pub async fn delete_memory(&self, name: &str) -> Result<()>
```

Direct memory manipulation. All acquire the write mutex.

### Consolidation

```rust
pub async fn consolidate(&self) -> Result<bool>
```

Attempt consolidation. Returns `Ok(true)` if it ran, `Ok(false)` if gated.

```rust
pub async fn consolidate_background(&self)
```

Spawn consolidation as a background task.

```rust
pub async fn record_session_end(&self)
```

Record a session ending. Increments the session counter and clears the surfaced memory set.

### Lifecycle

```rust
pub fn is_enabled(&self) -> bool
```

Check if memory is enabled (evaluates the enable/disable chain).

```rust
pub async fn shutdown(self)
```

Gracefully shut down. Aborts the extraction loop, waits for consolidation to finish.

---

## LlmProvider (trait)

```rust
pub trait LlmProvider: Send + Sync + 'static {
    fn complete(
        &self,
        messages: Vec<Message>,
        system: Option<String>,
    ) -> impl Future<Output = Result<String, MemoryError>> + Send + '_;
}
```

Implement this to connect any LLM. The library calls it for extraction, recall, and consolidation.

**Requirements:**
- `Send + Sync + 'static` — the provider is shared across async tasks
- The returned future must be `Send` — it runs on the tokio runtime
- Return the assistant's text response, or a `MemoryError::LlmError`

---

## MemoryConfig

```rust
pub struct MemoryConfig {
    pub memory_dir: PathBuf,
    pub max_index_lines: usize,          // 200
    pub max_index_bytes: usize,          // 25,600
    pub max_scan_files: usize,           // 200
    pub large_file_warning_bytes: u64,   // 40,960
    pub extraction_turn_interval: usize, // 1
    pub extraction_max_turns: usize,     // 5
    pub consolidation_cooldown: Duration,      // 24h
    pub consolidation_scan_throttle: Duration, // 10min
    pub consolidation_session_gate: usize,     // 5
    pub consolidation_lock_timeout: Duration,  // 60min
    pub consolidation_max_turns: usize,        // 30
    pub max_recall: usize,               // 5
    pub enabled_override: Option<bool>,  // None
    pub bare_mode: bool,                 // false
    pub remote_mode: bool,               // false
}
```

Construct via `MemoryConfig::builder(memory_dir).build()`. See [Configuration](./configuration.md) for details.

---

## Core Types

### Memory

```rust
pub struct Memory {
    pub name: String,
    pub description: String,
    pub memory_type: MemoryType,
    pub content: String,
    pub path: PathBuf,
    pub modified: Option<SystemTime>,
}
```

A single memory with its full content. `path` and `modified` are populated at load time.

### MemoryType

```rust
pub enum MemoryType { User, Feedback, Project, Reference }
```

### Message

```rust
pub struct Message {
    pub uuid: String,
    pub role: MessageRole,
    pub content: String,
}

pub enum MessageRole { User, Assistant, System }
```

Conversation messages passed to the extractor.

### ManifestEntry

```rust
pub struct ManifestEntry {
    pub title: String,
    pub filename: String,
    pub hook: String,
}
```

A single line in the MEMORY.md index.

### MemoryManifest

```rust
pub struct MemoryManifest {
    pub entries: Vec<ManifestEntry>,
    pub line_count: usize,
    pub byte_size: usize,
}
```

### RecallResult

```rust
pub struct RecallResult {
    pub memories: Vec<Memory>,
    pub filtered: Vec<String>,
}
```

### Staleness

```rust
pub enum Staleness {
    Fresh,
    Warning { age_days: u64 },
}
```

---

## MemoryError

```rust
pub enum MemoryError {
    Io { path: PathBuf, source: io::Error },
    FrontmatterParse { path: PathBuf, message: String },
    IndexCapExceeded { detail: String },
    NotFound { name: String },
    LlmError { message: String },
    LlmResponseParse { message: String },
    LockFailed { detail: String },
    ConsolidationLocked { pid: u32 },
    FileTooLarge { path: PathBuf, size_bytes: u64 },
    ScanLimitReached { count: usize, max: usize },
    Disabled { reason: String },
    Config { message: String },
}
```

All variants carry enough context to diagnose without unwrapping. `Io` always includes the file path.
