use std::path::PathBuf;
use thiserror::Error;

pub type Result<T> = std::result::Result<T, MemoryError>;

#[derive(Error, Debug)]
pub enum MemoryError {
    #[error("I/O error on {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("YAML frontmatter parse error in {path}: {message}")]
    FrontmatterParse { path: PathBuf, message: String },

    #[error("index cap exceeded: {detail}")]
    IndexCapExceeded { detail: String },

    #[error("memory not found: {name}")]
    NotFound { name: String },

    #[error("LLM call failed: {message}")]
    LlmError { message: String },

    #[error("LLM response could not be parsed: {message}")]
    LlmResponseParse { message: String },

    #[error("lock acquisition failed: {detail}")]
    LockFailed { detail: String },

    #[error("consolidation lock held by PID {pid}")]
    ConsolidationLocked { pid: u32 },

    #[error("file too large ({size_bytes} bytes): {path}")]
    FileTooLarge { path: PathBuf, size_bytes: u64 },

    #[error("scan limit reached: {count} files (max {max})")]
    ScanLimitReached { count: usize, max: usize },

    #[error("memory system disabled: {reason}")]
    Disabled { reason: String },

    #[error("configuration error: {message}")]
    Config { message: String },
}

impl MemoryError {
    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }
}
