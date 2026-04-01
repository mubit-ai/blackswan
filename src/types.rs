use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;
use std::time::SystemTime;

/// The four memory categories. Hard-coded taxonomy — no extension point.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MemoryType {
    User,
    Feedback,
    Project,
    Reference,
}

impl fmt::Display for MemoryType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::User => write!(f, "user"),
            Self::Feedback => write!(f, "feedback"),
            Self::Project => write!(f, "project"),
            Self::Reference => write!(f, "reference"),
        }
    }
}

/// A single memory, as parsed from a `.md` file with YAML frontmatter.
#[derive(Debug, Clone)]
pub struct Memory {
    pub name: String,
    pub description: String,
    pub memory_type: MemoryType,
    /// The markdown body below the frontmatter.
    pub content: String,
    /// Filesystem path (populated at load time).
    pub path: PathBuf,
    /// Last modified time (populated at load time).
    pub modified: Option<SystemTime>,
}

/// YAML frontmatter fields — used for serde round-tripping.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct MemoryFrontmatter {
    pub name: String,
    pub description: String,
    #[serde(rename = "type")]
    pub memory_type: MemoryType,
}

/// A lightweight entry in the MEMORY.md index.
#[derive(Debug, Clone)]
pub struct ManifestEntry {
    pub title: String,
    pub filename: String,
    pub hook: String,
}

impl fmt::Display for ManifestEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "- [{}]({}) — {}", self.title, self.filename, self.hook)
    }
}

/// The full manifest: contents of MEMORY.md parsed into entries.
#[derive(Debug, Clone, Default)]
pub struct MemoryManifest {
    pub entries: Vec<ManifestEntry>,
    pub line_count: usize,
    pub byte_size: usize,
}

/// A message in a conversation, passed to the extractor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub uuid: String,
    pub role: MessageRole,
    pub content: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    User,
    Assistant,
    System,
}

/// Result of a recall operation.
#[derive(Debug, Clone, Default)]
pub struct RecallResult {
    pub memories: Vec<Memory>,
    /// Filenames of memories that were considered but filtered (already surfaced).
    pub filtered: Vec<String>,
}

/// Staleness level for a memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Staleness {
    /// 0-1 days old — no warning.
    Fresh,
    /// 2+ days old — include age warning.
    Warning { age_days: u64 },
}
